//! The command bar: one floating input for native CAD commands and for requests to the assistant,
//! with the history, the active prompt (clickable keywords), suggestions and `/reference` menu.
//! Typing anywhere goes here (keyboard focus stays on the drawing). The rules for what Space, Enter,
//! Tab, Escape and the arrows do are in [`keyboard`] and [`crate::cmdinput`].

use aad_engine::Input;
use egui::{Color32, Key, Pos2, Rect, Sense, Stroke, pos2, vec2};

use crate::CadApp;
use crate::theme::Tokens;

#[derive(Default)]
pub struct CmdLine {
    pub buffer: String,
    /// The selection as of the last frame, to notice when it changes.
    seen_selection: Vec<aad_doc::Handle>,
    /// Something was typed at the command line since the selection was made.
    typed_since_selection: bool,
    pub history: Vec<String>,
    pub history_pos: Option<usize>,
    /// The expanded history window is open (F2, the chevron, or a click on the history lines).
    pub expanded: bool,
    /// Dragging the history's top edge: (pointer y, line count) when the drag started.
    resize_from: Option<(f32, usize)>,
    /// The highlighted row of the suggestion menu (arrow keys), if one was chosen.
    pub sug_sel: Option<usize>,
    /// Escape closed the suggestion menu for the text now in the bar.
    sug_hidden: bool,
    /// The text the two fields above belong to; typing something else resets them.
    sug_buf: String,
    /// An input method is composing text: Enter and Space belong to it, not to the bar.
    ime_composing: bool,
    /// Which references have nothing to read in the open drawing, with the reason: (document,
    /// revision, selection size) it was worked out for.
    ref_cache: Option<((u64, u64, usize), Vec<(String, String)>)>,
    /// How many rows the suggestion menu has this frame (the bar's hit zones keep clear of them).
    menu_rows: usize,
    /// Where the bar was drawn this frame, in screen points (for tests and hit-testing).
    pub bar: Option<Rect>,
}

/// The most history lines the command line shows above its bar.
pub const MAX_LINES: usize = 12;
const LINE_H: f32 = 16.0;

/// The Dynamic Input frame for the prompt on screen, or `None` when the value boxes are not live
/// (Dynamic Input off, no point prompt, a hot grip or a pending selection window).
pub fn dyn_frame(app: &CadApp) -> Option<crate::dyninput::Frame> {
    let st = &app.session.settings;
    if !st.dynmode || app.canvas.hot_grip.is_some() || app.session.pending_window.is_some() {
        return None;
    }
    let p = app.session.current_prompt().filter(|p| p.accept.point && !p.accept.select)?;
    Some(crate::dyninput::Frame { base: p.base, cartesian: st.dynpi_cartesian, absolute: st.dynpi_absolute })
}

/// Take Tab away from egui before it moves keyboard focus to the next widget (which would leave
/// the command line deaf to typing). While no widget has focus, a pressed Tab becomes a `"\t"`
/// text event, which keeps its place among the other keystrokes; [`keyboard`] handles it.
pub fn capture_tab(ctx: &egui::Context, raw: &mut egui::RawInput) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    raw.events.retain_mut(|e| {
        let tab = match e {
            egui::Event::Key { key: Key::Tab, pressed, .. } => Some(*pressed),
            _ => None,
        };
        match tab {
            Some(true) => {
                *e = egui::Event::Text("\t".into());
                true
            }
            Some(false) => false,
            None => true,
        }
    });
}

/// Tab on the command line: cycle the object snaps under the cursor, type the Dynamic Input
/// separator (`,` or `<`) after a value, or complete a command name.
fn tab(app: &mut CadApp) {
    // Over a snap marker with nothing typed, Tab cycles the candidate snaps.
    if app.cmd.buffer.is_empty() && app.canvas.snap.is_some() {
        app.canvas.snap_tab = true;
    } else if let Some(f) = dyn_frame(app)
        && let Some(sep) = crate::dyninput::tab_separator(&app.cmd.buffer, &f)
    {
        app.cmd.buffer.push(sep);
    } else if let Some(e) = {
        let m = menu(app);
        let i = app.cmd.sug_sel.unwrap_or(0).min(m.len().saturating_sub(1));
        m.get(i).cloned()
    } {
        accept_entry(app, &e);
    }
}

/// The references that have nothing to read in the open drawing, with the reason. Worked out again
/// only when the document, its revision or the selection size changes (it reads the whole drawing).
fn unavailable_refs(app: &mut CadApp) -> Vec<(String, String)> {
    let Ok(st) = app.session.state() else { return Vec::new() };
    let key = (st.uid, st.revision, st.selection.len());
    if let Some((k, v)) = &app.cmd.ref_cache
        && *k == key
    {
        return v.clone();
    }
    let mut out = Vec::new();
    for p in aad_engine::cmd::context::PROVIDERS {
        if let Err(aad_engine::cmd::context::RefError::Unavailable { reason, .. }) = aad_engine::cmd::context::resolve(&app.session, p.name) {
            out.push((p.name.to_string(), reason));
        }
    }
    app.cmd.ref_cache = Some((key, out.clone()));
    out
}

/// The rows of the suggestion menu for the text in the bar: references while a `/word` is being
/// typed (only when there is an assistant to give them to), command names otherwise. Empty when
/// The choices a running command's prompt offers (a room type, say) that match what is typed: the
/// text anywhere in the name or the note, case-insensitively, at most [`MAX_OPTION_ROWS`].
fn prompt_options(app: &CadApp) -> Vec<crate::cmdinput::Entry> {
    let Some(p) = app.session.current_prompt() else { return Vec::new() };
    let t = app.cmd.buffer.trim().to_lowercase();
    p.options
        .iter()
        .filter(|(name, note)| t.is_empty() || name.to_lowercase().contains(&t) || note.to_lowercase() == t)
        .take(MAX_OPTION_ROWS)
        .map(|(name, note)| crate::cmdinput::Entry { insert: name.clone(), label: note.clone(), dim: false, note: String::new(), reference: false })
        .collect()
}

/// Most rows of a prompt's choice list shown at once (typing narrows it).
const MAX_OPTION_ROWS: usize = 12;

/// none applies: a command is waiting for input, Escape closed the menu, or nothing matches.
pub fn menu(app: &mut CadApp) -> Vec<crate::cmdinput::Entry> {
    if app.session.running.is_some() {
        // A command waiting for a choice from a list shows the list.
        return if app.cmd.sug_hidden || app.canvas.hot_grip.is_some() { Vec::new() } else { prompt_options(app) };
    }
    if app.cmd.sug_hidden || app.canvas.hot_grip.is_some() {
        return Vec::new();
    }
    let buffer = app.cmd.buffer.clone();
    if crate::cmdinput::typing_reference(&buffer) {
        if !crate::assistant::available(app) {
            return Vec::new();
        }
        let un = unavailable_refs(app);
        crate::cmdinput::reference_entries(&buffer, &|n| un.iter().find(|(k, _)| k == n).map(|(_, r)| r.clone()))
    } else {
        crate::cmdinput::command_entries(&buffer)
    }
}

/// Put a suggestion in the bar without executing it (a reference gets a space after it).
fn accept_entry(app: &mut CadApp, e: &crate::cmdinput::Entry) {
    app.cmd.buffer = if e.reference { crate::cmdinput::accept_reference(&app.cmd.buffer, &e.insert) } else { e.insert.clone() };
    app.cmd.sug_sel = None;
}

impl CmdLine {
    /// Track the selection: a new selection starts with nothing typed since.
    pub fn watch_selection(&mut self, sel: &[aad_doc::Handle]) {
        if self.seen_selection != sel {
            self.seen_selection = sel.to_vec();
            self.typed_since_selection = false;
        }
    }

    /// Note that the command line holds typed text.
    pub fn note_typing(&mut self) {
        if !self.buffer.is_empty() {
            self.typed_since_selection = true;
        }
    }
}

/// Whether Backspace should erase the selection instead of editing the command line. The key
/// labelled "delete" on a Mac sends Backspace, so on macOS it erases selected objects, but only
/// when it can't be meant for the command line: a fresh, plain press (never an auto-repeat, so
/// holding it to clear typing can't run on into the drawing, and not Option/Cmd+delete, which
/// delete a word or line of text on macOS), no command running, the command line empty, and
/// nothing typed there since the selection was made.
pub fn backspace_erases(mac: bool, press: KeyPress, buffer: &str, command_running: bool, has_selection: bool, typed_since_selection: bool) -> bool {
    mac && press.plain && !press.repeat && buffer.is_empty() && !command_running && has_selection && !typed_since_selection
}

/// How a key was pressed.
#[derive(Clone, Copy, Debug)]
pub struct KeyPress {
    /// The operating system's auto-repeat while the key is held.
    pub repeat: bool,
    /// No Option, Control or Command held (Shift doesn't matter).
    pub plain: bool,
}

/// Handle keyboard input destined for the command line (when no text field has focus).
pub fn keyboard(app: &mut CadApp, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    let selection = app.session.selection();
    app.cmd.watch_selection(&selection);
    let mac = ctx.os() == egui::os::OperatingSystem::Mac;
    // A hot grip takes Space/Enter (cycle mode, or apply a typed point) and Escape.
    if app.canvas.hot_grip.is_some() {
        for ev in &events {
            match ev {
                egui::Event::Text(t) if t != " " && t != "\t" => app.cmd.buffer.push_str(t),
                egui::Event::Key { key: Key::Backspace, pressed: true, .. } => {
                    app.cmd.buffer.pop();
                }
                egui::Event::Key { key: Key::Escape, pressed: true, .. } => {
                    app.canvas.hot_grip = None;
                    app.cmd.buffer.clear();
                    app.session.echo("*Cancel*");
                }
                egui::Event::Key { key: Key::Enter | Key::Space, pressed: true, .. } => {
                    let typed = std::mem::take(&mut app.cmd.buffer);
                    if typed.trim().is_empty() {
                        if let Some(g) = app.canvas.hot_grip.as_mut() {
                            g.next_mode();
                            let l = g.label();
                            app.session.echo(l);
                        }
                    } else {
                        let base = app.canvas.hot_grip.map(|g| g.base).unwrap_or_default();
                        match aad_engine::prompt::parse_point_with(&typed, base, &app.session.angle_settings()) {
                            Some(p) => crate::canvas::apply_hot_grip(app, p),
                            None => app.session.echo("Requires a point (x,y, @dx,dy or @d<a)."),
                        }
                    }
                }
                _ => {}
            }
        }
        return;
    }
    let text_prompt = app.session.current_prompt().is_some_and(|p| p.accept.text && !p.accept.point && !p.accept.number && p.options.is_empty());
    let assistant = crate::assistant::state(app);
    // The assistant is asking Allow or Decline: the bar's prompt is that question until it is answered.
    if matches!(assistant, crate::assistant::State::Confirm { .. }) {
        for ev in events {
            let answer = match ev {
                egui::Event::Text(t) if t.eq_ignore_ascii_case("y") => Some(true),
                egui::Event::Text(t) if t.eq_ignore_ascii_case("n") => Some(false),
                // Enter takes the default, which is No; Escape declines.
                egui::Event::Key { key: Key::Enter | Key::Escape, pressed: true, .. } => Some(false),
                _ => None,
            };
            if let Some(yes) = answer {
                crate::assistant::answer(app, yes);
                break;
            }
        }
        return;
    }
    let sync_menu = |app: &mut CadApp| {
        if app.cmd.sug_buf != app.cmd.buffer {
            app.cmd.sug_buf = app.cmd.buffer.clone();
            app.cmd.sug_sel = None;
            app.cmd.sug_hidden = false;
        }
    };
    for ev in events {
        sync_menu(app);
        match ev {
            // Text from an input method: it is typed text, never a command trigger.
            egui::Event::Ime(egui::ImeEvent::Commit(t)) => {
                app.cmd.ime_composing = false;
                app.cmd.buffer.push_str(&t);
            }
            egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => app.cmd.ime_composing = !text.is_empty(),
            egui::Event::Text(t) => {
                if t == "\t" {
                    tab(app);
                } else if t == " " && !text_prompt && !app.cmd.ime_composing {
                    space(app);
                } else {
                    app.cmd.buffer.push_str(&t);
                }
            }
            // Keys the input method is using while composing are not the bar's.
            egui::Event::Key { pressed: true, .. } if app.cmd.ime_composing => {}
            egui::Event::Key { key, pressed: true, repeat, modifiers, .. } => match key {
                Key::Enter => enter(app),
                Key::Space if modifiers.ctrl || modifiers.command => {
                    // Ctrl+Space: open the suggestions (an empty bar opens the references).
                    app.cmd.sug_hidden = false;
                    if app.cmd.buffer.is_empty() && app.session.running.is_none() && crate::assistant::available(app) {
                        app.cmd.buffer.push('/');
                    }
                }
                Key::Escape => {
                    // First the menu closes, then a working assistant stops, then the usual cancel.
                    if !menu(app).is_empty() {
                        app.cmd.sug_hidden = true;
                        app.cmd.sug_sel = None;
                    } else if matches!(assistant, crate::assistant::State::Loading | crate::assistant::State::Thinking) {
                        crate::assistant::stop(app);
                    } else {
                        app.cmd.buffer.clear();
                        app.session.cancel();
                    }
                }
                Key::Backspace => {
                    let press = KeyPress { repeat, plain: !(modifiers.alt || modifiers.ctrl || modifiers.command) };
                    let erase = backspace_erases(
                        mac,
                        press,
                        &app.cmd.buffer,
                        app.session.running.is_some(),
                        !app.session.selection().is_empty(),
                        app.cmd.typed_since_selection,
                    );
                    if erase {
                        let _ = app.run("erase.selection", serde_json::json!({}));
                    } else {
                        app.cmd.buffer.pop();
                    }
                }
                // Up and Down move through the suggestions while a menu is open, else through the history.
                Key::ArrowUp | Key::ArrowDown if !modifiers.any() && !menu(app).is_empty() => {
                    let n = menu(app).len();
                    app.cmd.sug_sel = Some(match (app.cmd.sug_sel, key) {
                        (None, Key::ArrowUp) => n - 1,
                        (None, _) => 0,
                        (Some(i), Key::ArrowUp) => (i + n - 1) % n,
                        (Some(i), _) => (i + 1) % n,
                    });
                }
                Key::ArrowUp if !modifiers.any() && !app.cmd.history.is_empty() => {
                    let n = app.cmd.history.len();
                    let pos = app.cmd.history_pos.map(|p| p.saturating_sub(1)).unwrap_or(n - 1);
                    app.cmd.history_pos = Some(pos);
                    app.cmd.buffer = app.cmd.history.get(pos).cloned().unwrap_or_default();
                }
                Key::ArrowDown if !modifiers.any() => {
                    if let Some(p) = app.cmd.history_pos {
                        let np = p + 1;
                        if np >= app.cmd.history.len() {
                            app.cmd.history_pos = None;
                            app.cmd.buffer.clear();
                        } else {
                            app.cmd.history_pos = Some(np);
                            app.cmd.buffer = app.cmd.history.get(np).cloned().unwrap_or_default();
                        }
                    }
                }
                Key::Tab => tab(app),
                Key::Delete if app.cmd.buffer.is_empty() && app.session.running.is_none() => {
                    let _ = app.run("erase.selection", serde_json::json!({}));
                }
                _ => {}
            },
            _ => {}
        }
        app.cmd.note_typing();
    }
}

/// What Space or Enter picks from a running command's choice list: the row chosen with the arrows,
/// else (with text typed) the first match, unless the text already is one of the choices. `None`:
/// the text goes to the command as typed.
fn option_pick(app: &mut CadApp) -> Option<String> {
    app.session.running.as_ref()?;
    let m = menu(app);
    if m.is_empty() {
        return None;
    }
    if let Some(i) = app.cmd.sug_sel {
        return m.get(i).map(|e| e.insert.clone());
    }
    let t = app.cmd.buffer.trim().to_string();
    if t.is_empty() || m.iter().any(|e| e.insert.eq_ignore_ascii_case(&t)) {
        return None;
    }
    m.first().map(|e| e.insert.clone())
}

/// The command Space or Enter would start for a half-typed command name: the first suggestion in the
/// list under the bar (AutoCAD's habit). Only for a single word: a sentence is not a command, and an
/// assistant request starts with `/`. Not when the list was closed with Escape.
fn first_suggestion(app: &CadApp) -> Option<String> {
    let t = app.cmd.buffer.trim();
    if app.session.running.is_some() || app.cmd.sug_hidden || t.is_empty() || t.starts_with('/') || t.contains(char::is_whitespace) {
        return None;
    }
    crate::cmdinput::command_entries(t).into_iter().next().map(|e| e.insert)
}

/// Space. While a command is running it is Enter (the classic CAD habit). Otherwise it executes only
/// a whole resolved command (name or alias) or a suggestion that was chosen with the arrow keys, and
/// is an ordinary space in everything else: a request for the assistant keeps its spaces, and a half
/// typed word is never executed. An empty bar repeats the last command, as before.
fn space(app: &mut CadApp) {
    if let Some(pick) = option_pick(app) {
        app.cmd.buffer = pick;
        submit(app);
        return;
    }
    if app.session.running.is_some() || app.cmd.buffer.is_empty() {
        submit(app);
        return;
    }
    let chosen = app.cmd.sug_sel.and_then(|i| menu(app).get(i).cloned()).filter(|e| !e.reference);
    if let Some(e) = chosen {
        app.cmd.buffer = e.insert;
        submit(app);
    } else if crate::cmdinput::space_executes(&app.cmd.buffer, false) {
        // A whole name or alias runs under its command name, as Enter does, so the history matches.
        if let Some(name) = crate::cmdinput::exact_command(&app.cmd.buffer) {
            app.cmd.buffer = name;
        }
        submit(app);
    } else if let Some(first) = first_suggestion(app) {
        // A half-typed name: Space starts the first command in the list.
        app.cmd.buffer = first;
        submit(app);
    } else {
        app.cmd.buffer.push(' ');
    }
}

/// Enter. A chosen reference is put in the bar; a chosen command suggestion runs; otherwise the text
/// is submitted: a command to the engine, a request to the assistant. A request is kept in the bar
/// while the assistant is still busy with the last one.
fn enter(app: &mut CadApp) {
    if let Some(pick) = option_pick(app) {
        app.cmd.buffer = pick;
        submit(app);
        return;
    }
    if app.session.running.is_none()
        && let Some(e) = app.cmd.sug_sel.and_then(|i| menu(app).get(i).cloned())
    {
        if e.reference {
            accept_entry(app, &e);
        } else {
            app.cmd.buffer = e.insert;
            submit(app);
        }
        return;
    }
    // A half-typed command name: Enter starts the first command in the list.
    if let Some(first) = first_suggestion(app) {
        app.cmd.buffer = first;
        submit(app);
        return;
    }
    if app.session.running.is_none()
        && crate::cmdinput::is_ai(crate::cmdinput::classify(&app.cmd.buffer))
        && crate::assistant::available(app)
        && crate::assistant::state(app) != crate::assistant::State::Idle
    {
        app.session.echo(crate::tl!("The assistant is busy: wait, or press Escape to stop it."));
        return;
    }
    submit(app);
}

pub fn submit(app: &mut CadApp) {
    // Dynamic Input: a split entry (`40,30`, `40<90`, `40,`) is completed from the cursor and
    // measured from the last point.
    if let Some(f) = dyn_frame(app) {
        let cursor = app.canvas.cursor.unwrap_or(app.session.cursor);
        let last = app.session.last_point;
        let done = crate::dyninput::parse(&app.cmd.buffer, &f).and_then(|e| crate::dyninput::complete(&e, &f, cursor, last));
        if let Some(text) = done {
            app.cmd.buffer = text;
        }
    }
    let text = std::mem::take(&mut app.cmd.buffer);
    app.cmd.history_pos = None;
    if !text.trim().is_empty() && app.session.running.is_none() {
        app.cmd.history.push(text.clone());
        if app.cmd.history.len() > 200 {
            app.cmd.history.remove(0);
        }
    }
    if text.is_empty() {
        if let Err(e) = app.session.input(Input::Enter) {
            app.session.echo(e.to_string());
        }
        return;
    }
    app.cmdline(&text);
}

fn visible_lines(app: &CadApp, canvas: Rect, bar: Rect) -> usize {
    let available = ((bar.top() - canvas.top() - 8.0).max(0.0) / LINE_H) as usize;
    app.ui.history_lines.min(MAX_LINES).min(available)
}

/// The text in the bar with its meaning shown without a second field: a resolved command word and
/// `/references` in the accent colour (references underlined), everything else in the text colour.
fn coloured(buffer: &str, kind: crate::cmdinput::Kind, font: egui::FontId, t: &Tokens) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let mut first = true;
    let mut rest = buffer;
    while !rest.is_empty() {
        let space = rest.starts_with(char::is_whitespace);
        let end = rest.find(|c: char| c.is_whitespace() != space).unwrap_or(rest.len());
        let (tok, tail) = rest.split_at(end);
        let mut fmt = TextFormat { font_id: font.clone(), color: t.text, ..Default::default() };
        if !space {
            if crate::cmdinput::is_reference_word(tok) || (first && kind == crate::cmdinput::Kind::Native) {
                fmt.color = t.cmd_keyword;
            }
            if crate::cmdinput::is_reference_word(tok) {
                fmt.underline = Stroke::new(1.0, t.cmd_keyword);
            }
            first = false;
        }
        job.append(tok, 0.0, fmt);
        rest = tail;
    }
    job
}

pub fn show(app: &mut CadApp, ui: &mut egui::Ui, canvas: Rect) {
    let t = Tokens::get();
    keyboard(app, ui.ctx());
    let entries = menu(app);
    app.cmd.menu_rows = entries.len();
    if app.cmd.sug_sel.is_some_and(|i| i >= entries.len()) {
        app.cmd.sug_sel = None;
    }
    let assistant = crate::assistant::state(app);
    let busy_or_asking = assistant != crate::assistant::State::Idle;
    let kind = if app.session.running.is_none() { crate::cmdinput::classify(&app.cmd.buffer) } else { crate::cmdinput::Kind::Empty };
    // The same bar, two kinds of input: the mark at its left says which one is being typed.
    let ai_mode = crate::assistant::available(app) && (crate::cmdinput::is_ai(kind) || busy_or_asking);
    let w = (canvas.width() * 0.48).clamp(360.0, 760.0).min((canvas.width() - 8.0).max(0.0));
    let h = 24.0;
    let bar = Rect::from_min_size(pos2(canvas.center().x - w / 2.0, canvas.bottom() - h - 10.0), vec2(w, h));
    app.cmd.bar = Some(bar);
    let p = ui.painter_at(canvas);
    history(app, ui, canvas, bar);
    // History lines above the bar (the expanded window replaces them).
    let n = if app.cmd.expanded { 0 } else { visible_lines(app, canvas, bar) };
    let lines: Vec<String> = app.session.log.iter().rev().take(n).rev().cloned().collect();
    if !lines.is_empty() {
        let lh = 16.0;
        let hr = Rect::from_min_size(pos2(bar.left(), bar.top() - lh * lines.len() as f32 - 4.0), vec2(w, lh * lines.len() as f32 + 2.0));
        p.rect_filled(hr, 3.0, t.cmd_history);
        for (i, l) in lines.iter().enumerate() {
            p.text(pos2(hr.left() + 8.0, hr.top() + 1.0 + lh * i as f32), egui::Align2::LEFT_TOP, l, crate::theme::small(), t.text_dim);
        }
    }
    // The bar floats over the drawing: a soft shadow lifts it, and the accent outline shows it has input.
    let active = !app.cmd.buffer.is_empty() || app.session.running.is_some() || busy_or_asking;
    p.add(egui::epaint::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(80) }.as_shape(bar, 4.0));
    p.rect_filled(bar, 4.0, t.cmd_bg);
    p.rect_stroke(bar, 4.0, Stroke::new(if active { 1.5 } else { 1.0 }, if active { t.accent } else { t.cmd_border }), egui::StrokeKind::Inside);
    // Mode mark: `>_` for commands, an AI chip for requests to the assistant.
    if ai_mode {
        let chip = Rect::from_min_size(pos2(bar.left() + 5.0, bar.top() + 4.0), vec2(22.0, h - 8.0));
        p.rect_filled(chip, 3.0, t.accent);
        p.text(chip.center(), egui::Align2::CENTER_CENTER, "AI", crate::theme::small(), t.on_accent);
    } else {
        p.text(
            pos2(bar.left() + 8.0, bar.center().y),
            egui::Align2::LEFT_CENTER,
            ">_",
            crate::theme::mono(),
            if kind == crate::cmdinput::Kind::Native { t.accent } else { t.text_dim },
        );
    }
    let mut x = bar.left() + 30.0;
    let prompt = app.session.current_prompt();
    let font = crate::theme::body();
    if let Some(g) = app.canvas.hot_grip {
        let gl = p.layout_no_wrap(crate::i18n::t(g.label()).to_string(), font.clone(), t.text);
        let gw = gl.size().x;
        p.galley(pos2(x, bar.center().y - gl.size().y / 2.0), gl, t.text);
        x += gw + 6.0;
    } else if busy_or_asking {
        // The assistant's state is the prompt: a question for the user, or "working" with its way out.
        let dots = ".".repeat(((ui.input(|i| i.time) * 2.0) as usize % 3) + 1);
        match &assistant {
            crate::assistant::State::Confirm { tool, risk, summary } => {
                let lead = format!("{tool} ({risk}): {summary}  ");
                let g = p.layout_no_wrap(lead, font.clone(), t.text);
                let gw = g.size().x.min((bar.right() - x - 130.0).max(60.0));
                p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text);
                x += gw;
                for (i, (label, yes)) in [(crate::tl!("Yes"), true), (crate::tl!("No"), false)].into_iter().enumerate() {
                    let g = p.layout_no_wrap(label.to_string(), font.clone(), t.cmd_keyword);
                    let r = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(g.size().x + 4.0, h - 6.0));
                    let resp = ui.interact(r, ui.id().with(("ai_confirm", i)), Sense::click());
                    if resp.hovered() {
                        p.rect_filled(r, 2.0, t.cmd_keyword_hover);
                    }
                    p.galley(pos2(x + 2.0, bar.center().y - g.size().y / 2.0), g.clone(), t.cmd_keyword);
                    x += g.size().x + 10.0;
                    if resp.clicked() {
                        crate::assistant::answer(app, yes);
                    }
                }
                let g = p.layout_no_wrap(crate::tl!("<No>: ").to_string(), font.clone(), t.text_dim);
                let gw = g.size().x;
                p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text_dim);
                x += gw;
            }
            other => {
                let what = if matches!(other, crate::assistant::State::Loading) { crate::tl!("Loading the model") } else { crate::tl!("Thinking") };
                let g = p.layout_no_wrap(format!("{what}{dots} {} ", crate::tl!("(Esc to stop)")), font.clone(), t.text_dim);
                let gw = g.size().x;
                p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text_dim);
                x += gw;
            }
        }
    } else {
        match &prompt {
            Some(pr) => {
                let name = app.session.running.as_ref().map(|r| r.id.to_ascii_uppercase()).unwrap_or_default();
                let g = p.layout_no_wrap(format!("{name} "), font.clone(), t.text_faint);
                let gw = g.size().x;
                p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text_faint);
                x += gw;
                let g = p.layout_no_wrap(crate::i18n::t(&pr.message).to_string(), font.clone(), t.text);
                let gw = g.size().x;
                p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text);
                x += gw;
                if !pr.keywords.is_empty() {
                    let g = p.layout_no_wrap(if pr.message.is_empty() { " [".into() } else { crate::tl!(" or [").into() }, font.clone(), t.text);
                    let gw = g.size().x;
                    p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text);
                    x += gw;
                    let kws = pr.keywords.clone();
                    for (i, k) in kws.iter().enumerate() {
                        let g = p.layout_no_wrap(k.clone(), font.clone(), t.cmd_keyword);
                        let r = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(g.size().x, h - 6.0));
                        let resp = ui.interact(r, ui.id().with(("kw", i)), Sense::click());
                        if resp.hovered() {
                            p.rect_filled(r, 2.0, t.cmd_keyword_hover);
                        }
                        let gw = g.size().x;
                        p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.cmd_keyword);
                        x += gw;
                        if resp.clicked() {
                            let _ = app.session.input(Input::Keyword(k.clone()));
                        }
                        if i + 1 < kws.len() {
                            let g = p.layout_no_wrap("/".into(), font.clone(), t.text);
                            let gw = g.size().x;
                            p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text);
                            x += gw;
                        }
                    }
                    let g = p.layout_no_wrap("]".into(), font.clone(), t.text);
                    let gw = g.size().x;
                    p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text);
                    x += gw;
                }
                if let Some(d) = &pr.default {
                    let g = p.layout_no_wrap(format!(" <{d}>"), font.clone(), t.text);
                    let gw = g.size().x;
                    p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text);
                    x += gw;
                }
                let g = p.layout_no_wrap(": ".into(), font.clone(), t.text);
                let gw = g.size().x;
                p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text);
                x += gw;
            }
            None => {
                if app.cmd.buffer.is_empty() {
                    p.text(
                        pos2(x, bar.center().y),
                        egui::Align2::LEFT_CENTER,
                        if crate::assistant::available(app) {
                            crate::tl!("Type a command, or / to ask the assistant")
                        } else {
                            crate::tl!("Type a command")
                        },
                        egui::FontId::proportional(12.5),
                        t.text_faint,
                    );
                }
            }
        }
    }
    let g = p.layout_job(coloured(&app.cmd.buffer, kind, font, &t));
    let gw = g.size().x;
    p.galley(pos2(x, bar.center().y - g.size().y / 2.0), g, t.text);
    // Caret.
    let blink = (ui.input(|i| i.time) * 2.0).floor() as i64 % 2 == 0;
    if blink {
        p.line_segment([pos2(x + gw + 1.0, bar.top() + 5.0), pos2(x + gw + 1.0, bar.bottom() - 5.0)], Stroke::new(1.0, t.text));
    }
    // History chevron.
    crate::icons::paint(
        &p,
        Rect::from_center_size(pos2(bar.right() - 14.0, bar.center().y), vec2(14.0, 14.0)),
        if app.cmd.expanded { crate::icons::Icon::ChevronDown } else { crate::icons::Icon::ChevronUp },
        false,
    );
    // Suggestions: command names, or the `/references` with what each has to read in this drawing.
    if !entries.is_empty() {
        let rh = 22.0;
        let above = if lines.is_empty() { 0.0 } else { 16.0 * lines.len() as f32 + 6.0 };
        let lr = Rect::from_min_size(
            pos2(bar.left() + 24.0, bar.top() - 6.0 - rh * entries.len() as f32 - above),
            vec2((bar.width() - 24.0).max(0.0), rh * entries.len() as f32),
        );
        p.add(egui::epaint::Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(80) }.as_shape(lr, 4.0));
        p.rect_filled(lr, 4.0, t.list_bg);
        p.rect_stroke(lr, 4.0, Stroke::new(1.0, t.cmd_border), egui::StrokeKind::Inside);
        let chosen = app.cmd.sug_sel;
        for (i, e) in entries.iter().enumerate() {
            let r = Rect::from_min_size(pos2(lr.left(), lr.top() + rh * i as f32), vec2(lr.width(), rh));
            let resp = ui.interact(r, ui.id().with(("sug", i)), Sense::click());
            if chosen == Some(i) {
                p.rect_filled(r.shrink(1.0), 3.0, t.select_bg);
                p.rect_filled(Rect::from_min_size(r.min + vec2(1.0, 3.0), vec2(2.0, rh - 6.0)), 1.0, t.accent);
            } else if resp.hovered() || (chosen.is_none() && i == 0) {
                p.rect_filled(r.shrink(1.0), 3.0, if resp.hovered() { t.cmd_keyword_hover } else { t.list_row });
            }
            let ink = if e.dim { t.text_faint } else { t.text };
            p.text(Pos2::new(r.left() + 10.0, r.center().y), egui::Align2::LEFT_CENTER, &e.insert, crate::theme::body(), ink);
            // A reference with nothing to read says why, in words (not only dimmed).
            let right = if e.dim { e.note.clone() } else { crate::i18n::t(&e.label).to_string() };
            p.text(Pos2::new(r.right() - 8.0, r.center().y), egui::Align2::RIGHT_CENTER, right, crate::theme::small(), t.text_faint);
            if resp.clicked() {
                if app.session.running.is_some() {
                    // A choice for the running command (a room type): enter it.
                    app.cmd.buffer = e.insert.clone();
                    submit(app);
                } else if e.reference {
                    accept_entry(app, e);
                } else {
                    app.cmd.buffer.clear();
                    app.start(&e.insert.to_ascii_lowercase());
                }
            }
        }
    }
}

/// The command line's areas, laid out like [`show`] draws them.
struct Zones {
    /// The inline history lines (when there are any and the window is closed).
    lines: Option<Rect>,
    /// The expanded history window.
    window: Option<Rect>,
    chevron: Rect,
    /// The top edge that resizes the inline history.
    grip: Rect,
    /// Everything, AutoComplete list included: kept away from the drawing underneath.
    all: Rect,
}

fn zones(app: &CadApp, canvas: Rect, bar: Rect) -> Zones {
    let n = if app.cmd.expanded { 0 } else { visible_lines(app, canvas, bar).min(app.session.log.len()) };
    let lines =
        (n > 0).then(|| Rect::from_min_size(pos2(bar.left(), bar.top() - LINE_H * n as f32 - 4.0), vec2(bar.width(), LINE_H * n as f32 + 2.0)));
    let window = app.cmd.expanded.then(|| {
        // As tall as the log (at least three lines), at most half the drawing area.
        let full = LINE_H * app.session.log.len().max(3) as f32 + 4.0;
        let h = full.min((canvas.height() * 0.5).max(3.0 * LINE_H));
        Rect::from_min_size(pos2(bar.left(), bar.top() - h - 4.0), vec2(bar.width(), h + 2.0))
    });
    let top = window.or(lines).map_or(bar.top(), |r| r.top());
    let grip = Rect::from_min_max(pos2(bar.left(), top - 3.0), pos2(bar.right(), top + 3.0));
    let mut all = bar.union(grip);
    {
        let k = app.cmd.menu_rows;

        if k > 0 {
            let above = if n == 0 { 0.0 } else { LINE_H * n as f32 + 6.0 };
            all = all.union(Rect::from_min_size(pos2(bar.left(), bar.top() - 6.0 - 22.0 * k as f32 - above), vec2(bar.width(), 1.0)));
        }
    }
    Zones { lines, window, chevron: Rect::from_center_size(pos2(bar.right() - 14.0, bar.center().y), vec2(24.0, 22.0)), grip, all }
}

/// The pointer on the command line: one widget under all of it keeps clicks, drags and the wheel
/// off the drawing (the keywords and AutoComplete rows sit on top of it). Clicking the history
/// lines or the chevron opens/closes the history window (`ui.toggle.history`, also F2); dragging
/// the top edge sets how many lines show (`ui.cmdline.lines`).
fn history(app: &mut CadApp, ui: &mut egui::Ui, canvas: Rect, bar: Rect) {
    let z = zones(app, canvas, bar);
    let resp = ui.interact(z.all, ui.id().with("cmdline"), Sense::click_and_drag());
    let at = resp.hover_pos().or(resp.interact_pointer_pos());
    let on_grip = |p: Pos2| z.grip.contains(p) && z.window.is_none();
    let on_toggle = |p: Pos2| z.chevron.contains(p) || z.lines.is_some_and(|r| r.contains(p));
    if app.cmd.resize_from.is_some() || at.is_some_and(on_grip) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    } else if at.is_some_and(on_toggle) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.drag_started()
        && let Some(o) = ui.input(|i| i.pointer.press_origin())
        && on_grip(o)
    {
        app.cmd.resize_from = Some((o.y, visible_lines(app, canvas, bar)));
    }
    if let Some((y0, n0)) = app.cmd.resize_from {
        if let (true, Some(p)) = (resp.dragged(), resp.interact_pointer_pos()) {
            let n = (n0 as f32 + (y0 - p.y) / LINE_H).round().clamp(0.0, MAX_LINES as f32) as usize;
            if n != app.ui.history_lines {
                let _ = app.run("ui.cmdline.lines", serde_json::json!({ "lines": n }));
            }
        } else {
            app.cmd.resize_from = None;
        }
    }
    if resp.clicked() && resp.interact_pointer_pos().is_some_and(on_toggle) {
        let _ = app.run("ui.toggle.history", serde_json::json!({}));
    }
    if let Some(r) = zones(app, canvas, bar).window
        && history_window(app, ui, r)
    {
        let _ = app.run("ui.history.clear", serde_json::json!({}));
    }
}

/// The expanded history: the whole command log, scrollable, newest at the bottom.
/// Draws it; returns true when its Clear button was clicked.
fn history_window(app: &CadApp, ui: &mut egui::Ui, r: Rect) -> bool {
    let t = Tokens::get();
    ui.painter().rect_filled(r, 3.0, t.cmd_bg);
    ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let inner = r.shrink2(vec2(8.0, 3.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    child.spacing_mut().item_spacing.y = 0.0;
    let log = &app.session.log;
    egui::ScrollArea::vertical().id_salt("cmd_history").stick_to_bottom(true).auto_shrink([false, false]).show_rows(
        &mut child,
        LINE_H,
        log.len(),
        |ui, rows| {
            for l in log.get(rows).into_iter().flatten() {
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), LINE_H), Sense::hover());
                let p = ui.painter().with_clip_rect(row.intersect(ui.clip_rect()));
                p.text(pos2(row.left(), row.center().y), egui::Align2::LEFT_CENTER, l, crate::theme::small(), t.text_dim);
            }
        },
    );
    // A small Clear button in the corner: forgets the command log and the typed commands.
    let clear = Rect::from_min_size(pos2(r.right() - 58.0, r.top() + 4.0), vec2(52.0, 18.0));
    let resp = ui.interact(clear, ui.id().with("cmd_history_clear"), Sense::click());
    let hot = resp.hovered();
    ui.painter().rect_filled(clear, 3.0, if hot { t.control_hover } else { t.control });
    ui.painter().text(clear.center(), egui::Align2::CENTER_CENTER, crate::tl!("Clear"), crate::theme::small(), t.text);
    resp.on_hover_text(crate::tl!("Clear the command history")).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aad_engine::Session;
    use egui::{Event, PointerButton};

    #[test]
    fn compact_canvas_contains_history_controls_without_changing_line_preference() {
        let mut app = CadApp::new(Session::empty(), crate::Services::default());
        app.ui.history_lines = 12;
        for i in 0..20 {
            app.session.echo(format!("line {i}"));
        }
        let canvas = Rect::from_min_size(Pos2::ZERO, vec2(300.0, 180.0));
        let w = (canvas.width() * 0.48).clamp(360.0, 760.0).min((canvas.width() - 8.0).max(0.0));
        let bar = Rect::from_min_size(pos2(canvas.center().x - w / 2.0, canvas.bottom() - 34.0), vec2(w, 24.0));
        let z = zones(&app, canvas, bar);
        assert!(canvas.contains_rect(bar));
        assert!(canvas.contains_rect(z.chevron));
        assert!(z.lines.is_some_and(|r| canvas.contains_rect(r)));
        assert!(canvas.contains_rect(z.grip));
        assert_eq!(app.ui.history_lines, 12);
    }

    fn frame(app: &mut CadApp, ctx: &egui::Context, events: Vec<Event>) {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        out.textures_delta.clear();
    }

    fn press(app: &mut CadApp, ctx: &egui::Context, at: Pos2, pressed: bool) {
        frame(app, ctx, vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: egui::Modifiers::default() }]);
    }

    #[test]
    fn history_expands_by_click_chevron_f2_and_resizes_by_drag_without_touching_the_drawing() {
        let mut app = CadApp::new(Session::empty(), crate::Services::default());
        app.start("ui.sample");
        for i in 0..20 {
            app.session.echo(format!("line {i}"));
        }
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Vec::new());
        frame(&mut app, &ctx, Vec::new());
        let canvas = app.canvas.rect.unwrap_or(Rect::NOTHING);
        let w = (canvas.width() * 0.48).clamp(360.0, 760.0).min((canvas.width() - 8.0).max(0.0));
        let bar = Rect::from_min_size(pos2(canvas.center().x - w / 2.0, canvas.bottom() - 34.0), vec2(w, 24.0));
        let z = zones(&app, canvas, bar);
        let Some(lines) = z.lines else { panic!("three history lines show by default") };
        // A click on the history lines opens the history window, and doesn't start a selection.
        for (at, pressed) in [(lines.center(), true), (lines.center(), false)] {
            frame(&mut app, &ctx, vec![Event::PointerMoved(at)]);
            press(&mut app, &ctx, at, pressed);
        }
        frame(&mut app, &ctx, Vec::new());
        assert!(app.cmd.expanded);
        assert!(app.session.pending_window.is_none(), "the click stayed on the command line");
        // The chevron closes it; F2 and TEXTSCR open it.
        let chevron = z.chevron.center();
        press(&mut app, &ctx, chevron, true);
        press(&mut app, &ctx, chevron, false);
        frame(&mut app, &ctx, Vec::new());
        assert!(!app.cmd.expanded);
        let f2 = |pressed| Event::Key { key: Key::F2, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE };
        frame(&mut app, &ctx, vec![f2(true), f2(false)]);
        assert!(app.cmd.expanded);
        assert_eq!(app.run("ui.toggle.history", serde_json::json!({"on": false})), Ok(serde_json::json!({"on": false})));
        app.cmdline("textscr");
        assert!(app.cmd.expanded);
        let _ = app.run("ui.toggle.history", serde_json::json!({"on": false}));
        frame(&mut app, &ctx, Vec::new());
        // Dragging the top edge up by two lines shows two more lines (capped).
        let top = pos2(lines.center().x, lines.top() - 1.0);
        frame(&mut app, &ctx, vec![Event::PointerMoved(top)]);
        press(&mut app, &ctx, top, true);
        for dy in [8.0, 20.0, 2.0 * LINE_H] {
            frame(&mut app, &ctx, vec![Event::PointerMoved(top - vec2(0.0, dy))]);
        }
        press(&mut app, &ctx, top - vec2(0.0, 2.0 * LINE_H), false);
        frame(&mut app, &ctx, Vec::new());
        assert_eq!(app.ui.history_lines, 4, "two lines by default, two more by the drag");
        assert!(!app.cmd.expanded, "a drag isn't a click");
        assert!(app.session.pending_window.is_none());
        assert_eq!(app.run("ui.cmdline.lines", serde_json::json!({"lines": 99})), Ok(serde_json::json!({"lines": MAX_LINES})));
    }
}

#[cfg(test)]
mod delete_key_tests {
    use super::*;
    use aad_doc::Handle;

    const PRESS: KeyPress = KeyPress { repeat: false, plain: true };

    #[test]
    fn mac_delete_erases_a_fresh_selection() {
        assert!(backspace_erases(true, PRESS, "", false, true, false));
        // Not on Windows/Linux, where Backspace never deletes objects.
        assert!(!backspace_erases(false, PRESS, "", false, true, false));
        // Nothing selected, a command running, or text on the command line: just edit text.
        assert!(!backspace_erases(true, PRESS, "", false, false, false));
        assert!(!backspace_erases(true, PRESS, "", true, true, false));
        assert!(!backspace_erases(true, PRESS, "m", false, true, false));
        // Option/Cmd+delete are text-editing gestures on macOS.
        assert!(!backspace_erases(true, KeyPress { plain: false, ..PRESS }, "", false, true, false));
    }

    #[test]
    fn holding_delete_to_clear_typing_never_erases() {
        // Auto-repeat after the last character is gone.
        assert!(!backspace_erases(true, KeyPress { repeat: true, ..PRESS }, "", false, true, false));
        // Typed, then cleared with separate presses: still not erased.
        assert!(!backspace_erases(true, PRESS, "", false, true, true));
    }

    #[test]
    fn typing_is_forgotten_when_the_selection_changes() {
        let mut c = CmdLine::default();
        c.watch_selection(&[Handle(1)]);
        c.buffer.push('m');
        c.note_typing();
        c.buffer.clear();
        c.watch_selection(&[Handle(1)]);
        assert!(c.typed_since_selection);
        c.watch_selection(&[Handle(1), Handle(2)]);
        assert!(!c.typed_since_selection);
    }
}
