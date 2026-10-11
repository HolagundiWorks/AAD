//! Floating widgets: the architect calculator and the Pomodoro timer.
//!
//! Both are thin: the maths and the timer live in `cadcraft_engine::{calc, pomodoro}`; the windows
//! only draw them and send key presses through commands (`ui.calculator`, `ui.pomodoro`), so the
//! control channel and the tests drive exactly what the buttons do. The timer keeps counting while
//! its window is closed.

use cadcraft_engine::calc::{Calculator, Kind};
use cadcraft_engine::pomodoro::{Event, Pomodoro};
use egui::{Align2, Color32, Pos2, RichText, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// Runtime state of the widgets (not saved: the calculator starts clean, the timer fresh).
#[derive(Default)]
pub struct WidgetState {
    pub calc: Calculator,
    pub pomodoro: Pomodoro,
    /// An AQC file read for preview, waiting for Apply or Cancel.
    pub bridge: Option<BridgePending>,
    /// The open input dialog of a command ([`crate::formdialog`]).
    pub form: Option<crate::formdialog::State>,
    last_time: Option<f64>,
    carry: f64,
}

/// Routes `ui.calculator`, `ui.pomodoro`, `ui.toggle.calculator` and `ui.toggle.pomodoro`.
pub fn route(app: &mut CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    let on = |b: &mut bool| *b = params.get("on").and_then(Value::as_bool).unwrap_or(!*b);
    Some(match id {
        "ui.toggle.calculator" => {
            on(&mut app.ui.show_calculator);
            Ok(json!({ "on": app.ui.show_calculator }))
        }
        "ui.toggle.pomodoro" => {
            on(&mut app.ui.show_pomodoro);
            Ok(json!({ "on": app.ui.show_pomodoro }))
        }
        "ui.calculator" => Ok(calculator_command(app, params)),
        "ui.pomodoro" => pomodoro_command(app, params),
        _ => return None,
    })
}

fn calculator_command(app: &mut CadApp, p: &Value) -> Value {
    let c = &mut app.widgets.calc;
    match p.get("action").and_then(Value::as_str) {
        Some("clear") => c.clear(),
        Some("backspace") => c.backspace(),
        Some("equals") => c.evaluate(),
        _ => {}
    }
    if let Some(keys) = p.get("keys").and_then(Value::as_str) {
        // Anything after the cap is dropped, whatever the caller sent.
        for ch in keys.chars().take(cadcraft_engine::calc::MAX_EXPRESSION * 2) {
            c.type_char(ch);
        }
    }
    let r = c.results();
    json!({
        "display": c.display(),
        "expression": c.raw(),
        "summary": c.expression_summary(),
        "kind": c.kind().name(),
        "results": { "m": r.metres, "m2": r.square_metres, "m3": r.cubic_metres, "ft": r.feet, "ft2": r.square_feet, "ft3": r.cubic_feet },
    })
}

fn pomodoro_command(app: &mut CadApp, p: &Value) -> Result<Value, String> {
    let mut note = None;
    {
        let t = &mut app.widgets.pomodoro;
        match p.get("action").and_then(Value::as_str) {
            None | Some("status") => {}
            Some("toggle") => t.toggle(),
            Some("start") => {
                if !t.running() {
                    t.toggle();
                }
            }
            Some("pause") => {
                if t.running() {
                    t.toggle();
                }
            }
            Some("reset") => t.reset(),
            Some("set") => {
                let m = p.get("minutes").and_then(Value::as_u64).ok_or("`minutes` is required (5 to 60)")?;
                let m = t.set_minutes(u32::try_from(m).unwrap_or(u32::MAX));
                note = Some(format!("Pomodoro set to {m} min"));
            }
            Some(other) => return Err(format!("unknown action `{other}`: use toggle, start, pause, reset, set or status")),
        }
    }
    if let Some(n) = note {
        app.set_status(n);
    }
    let t = &app.widgets.pomodoro;
    Ok(json!({ "label": t.label(), "running": t.running(), "left": t.left(), "minutes": t.duration() / 60 }))
}

/// Called once per frame: counts the timer down (even when its window is hidden), then draws the
/// open widgets.
pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    advance_timer(app, ctx);
    crate::formdialog::show(app, ctx);
    if app.widgets.bridge.is_some() {
        bridge_window(app, ctx);
    }
    if app.ui.show_calculator {
        calculator_window(app, ctx);
    }
    if app.ui.show_pomodoro {
        pomodoro_window(app, ctx);
    }
}

fn advance_timer(app: &mut CadApp, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    let w = &mut app.widgets;
    let dt = w.last_time.map_or(0.0, |t| (now - t).max(0.0));
    w.last_time = Some(now);
    if !w.pomodoro.running() {
        w.carry = 0.0;
        return;
    }
    w.carry += dt;
    let whole = w.carry.floor();
    w.carry -= whole;
    let done = w.pomodoro.tick(whole.min(f64::from(u32::MAX)) as u32) == Event::Completed;
    if done {
        app.ui.show_pomodoro = true;
        app.set_status(crate::tl!("Pomodoro: focus block complete"));
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
}

// ---------------- calculator ----------------

fn calculator_window(app: &mut CadApp, ctx: &egui::Context) {
    let t = Tokens::get();
    let mut open = true;
    let screen = ctx.content_rect();
    let resp = egui::Window::new(crate::tl!("Calculator"))
        .id(egui::Id::new("widget_calculator"))
        .open(&mut open)
        .resizable(false)
        .collapsible(true)
        .default_pos(Pos2::new((screen.right() - 290.0).max(0.0), 90.0))
        .show(ctx, |ui| {
            ui.set_width(250.0);
            let c = &app.widgets.calc;
            let (summary, display, kind) = (c.expression_summary(), c.display(), c.kind());
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                ui.label(RichText::new(if summary.is_empty() { " ".to_string() } else { summary }).small().color(t.text_dim));
                ui.label(RichText::new(display).size(26.0).monospace().color(t.text));
                let label = match kind {
                    Kind::Length => crate::tl!("Length"),
                    Kind::Area => crate::tl!("Area"),
                    Kind::Volume => crate::tl!("Volume"),
                };
                ui.label(RichText::new(label).small().color(t.accent));
            });
            ui.add_space(4.0);
            keypad(app, ui, &t);
            ui.add_space(6.0);
            results(app, ui, &t);
        });
    // Typing goes to the calculator while the pointer is over it and nothing else has the keyboard.
    if resp.is_some_and(|r| r.response.contains_pointer()) && ctx.memory(|m| m.focused()).is_none() {
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            match e {
                egui::Event::Text(s) => {
                    let _ = app.run("ui.calculator", json!({ "keys": s }));
                }
                egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => {
                    let _ = app.run("ui.calculator", json!({ "action": "equals" }));
                }
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                    let _ = app.run("ui.calculator", json!({ "action": "backspace" }));
                }
                egui::Event::Key { key: egui::Key::Escape, pressed: true, .. } => {
                    let _ = app.run("ui.calculator", json!({ "action": "clear" }));
                }
                _ => {}
            }
        }
    }
    if !open {
        app.ui.show_calculator = false;
    }
}

fn keypad(app: &mut CadApp, ui: &mut Ui, t: &Tokens) {
    // (label, keys or action, kind): 0 digit, 1 operator, 2 function, 3 equals
    const ROWS: [[(&str, &str, u8); 4]; 6] = [
        [("C", "@clear", 2), ("⌫", "@backspace", 2), ("(", "(", 2), (")", ")", 2)],
        [("7", "7", 0), ("8", "8", 0), ("9", "9", 0), ("÷", "/", 1)],
        [("4", "4", 0), ("5", "5", 0), ("6", "6", 0), ("×", "x", 1)],
        [("1", "1", 0), ("2", "2", 0), ("3", "3", 0), ("−", "-", 1)],
        [("0", "0", 0), (".", ".", 0), ("′", "'", 2), ("″", "\"", 2)],
        [("^", "^", 1), ("+", "+", 1), ("=", "@equals", 3), ("=", "@equals", 3)],
    ];
    let size = vec2(56.0, 34.0);
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    for row in ROWS {
        ui.horizontal(|ui| {
            // The last row's equals key spans two columns.
            let mut skip_next = false;
            for (n, (label, keys, kind)) in row.into_iter().enumerate() {
                if skip_next {
                    skip_next = false;
                    continue;
                }
                let (fill, text) = match kind {
                    0 => (t.control, t.text),
                    1 => (t.control_hover, t.accent),
                    3 => (t.accent, Color32::WHITE),
                    _ => (t.control_hover, t.text),
                };
                let wide = kind == 3 && n == 2;
                skip_next = wide;
                let sz = if wide { vec2(size.x * 2.0 + 4.0, size.y) } else { size };
                let b = egui::Button::new(RichText::new(label).size(16.0).color(text)).fill(fill);
                if ui.add_sized(sz, b).clicked() {
                    let params = match keys.strip_prefix('@') {
                        Some(action) => json!({ "action": action }),
                        None => json!({ "keys": keys }),
                    };
                    let _ = app.run("ui.calculator", params);
                }
            }
        });
    }
}

fn results(app: &mut CadApp, ui: &mut Ui, t: &Tokens) {
    let r = app.widgets.calc.results();
    egui::Grid::new("calc_results").num_columns(2).spacing(vec2(16.0, 2.0)).min_col_width(110.0).show(ui, |ui| {
        for (a, b) in [
            (format!("{} m", r.metres), format!("{} ft", r.feet)),
            (format!("{} m²", r.square_metres), format!("{} ft²", r.square_feet)),
            (format!("{} m³", r.cubic_metres), format!("{} ft³", r.cubic_feet)),
        ] {
            ui.label(RichText::new(a).monospace().color(t.text));
            ui.label(RichText::new(b).monospace().color(t.text_dim));
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    if ui.small_button(crate::tl!("Copy result")).clicked() {
        let text = app.widgets.calc.display();
        ui.ctx().copy_text(text);
        app.set_status(crate::tl!("Result copied"));
    }
}

// ---------------- pomodoro ----------------

fn pomodoro_window(app: &mut CadApp, ctx: &egui::Context) {
    let t = Tokens::get();
    let mut open = true;
    let screen = ctx.content_rect();
    egui::Window::new(crate::tl!("Pomodoro"))
        .id(egui::Id::new("widget_pomodoro"))
        .open(&mut open)
        .resizable(false)
        .collapsible(true)
        .default_pos(Pos2::new((screen.right() - 190.0).max(0.0), (screen.bottom() - 330.0).max(60.0)))
        .show(ctx, |ui| {
            ui.set_width(150.0);
            dial(app, ui, &t);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let running = app.widgets.pomodoro.running();
                let go = if running { crate::tl!("Pause") } else { crate::tl!("Start") };
                if ui.button(go).clicked() {
                    let _ = app.run("ui.pomodoro", json!({ "action": "toggle" }));
                }
                if ui.button(crate::tl!("Reset")).clicked() {
                    let _ = app.run("ui.pomodoro", json!({ "action": "reset" }));
                }
            });
            ui.horizontal(|ui| {
                let m = app.widgets.pomodoro.duration() / 60;
                ui.add_enabled_ui(!app.widgets.pomodoro.running(), |ui| {
                    if ui.small_button("−5").on_hover_text(crate::tl!("Shorter block")).clicked() {
                        let _ = app.run("ui.pomodoro", json!({ "action": "set", "minutes": m.saturating_sub(5) }));
                    }
                    ui.label(RichText::new(format!("{m} min")).small().color(t.text_dim));
                    if ui.small_button("+5").on_hover_text(crate::tl!("Longer block")).clicked() {
                        let _ = app.run("ui.pomodoro", json!({ "action": "set", "minutes": m + 5 }));
                    }
                });
            });
        });
    if !open {
        app.ui.show_pomodoro = false;
    }
}

/// The dial: a ring that empties as time runs out, the time in the middle. Click starts or pauses,
/// double click resets, and dragging it while stopped sets the length.
fn dial(app: &mut CadApp, ui: &mut Ui, t: &Tokens) {
    let size = 130.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let c = rect.center();
    let r = size / 2.0 - 8.0;
    let p = &app.widgets.pomodoro;
    let painter = ui.painter_at(rect);
    painter.circle_stroke(c, r, Stroke::new(6.0, t.border));
    let frac = p.fraction();
    if frac > 0.001 {
        let steps = ((frac * 120.0).ceil() as usize).max(2);
        let pts: Vec<Pos2> = (0..=steps)
            .map(|k| {
                let a = -std::f32::consts::FRAC_PI_2 + (frac as f32) * std::f32::consts::TAU * (k as f32 / steps as f32);
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        let color = if p.running() || p.dragging() { t.accent } else { t.text_dim };
        painter.add(egui::Shape::line(pts, Stroke::new(6.0, color)));
    }
    // The 12 o'clock mark and the dial's 5-minute ticks.
    for k in 0..12 {
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 12.0;
        let (d, len) = (vec2(a.cos(), a.sin()), if k % 3 == 0 { 7.0 } else { 4.0 });
        painter.line_segment([c + d * (r - 10.0), c + d * (r - 10.0 - len)], Stroke::new(1.0, t.text_faint));
    }
    painter.text(c - vec2(0.0, 6.0), Align2::CENTER_CENTER, p.label(), egui::FontId::monospace(28.0), t.text);
    let state = if p.dragging() {
        crate::tl!("Set length")
    } else if p.running() {
        crate::tl!("Focus")
    } else if p.left() == 0 {
        crate::tl!("Done")
    } else if p.active() {
        crate::tl!("Paused")
    } else {
        crate::tl!("Ready")
    };
    painter.text(c + vec2(0.0, 20.0), Align2::CENTER_CENTER, state, egui::FontId::proportional(12.0), t.text_dim);

    if resp.dragged()
        && !app.widgets.pomodoro.running()
        && let Some(pt) = resp.interact_pointer_pos()
    {
        let d = pt - c;
        let turn = f64::from((d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2) / std::f32::consts::TAU);
        app.widgets.pomodoro.drag_to(turn);
    }
    if resp.drag_stopped()
        && let Some(m) = app.widgets.pomodoro.end_drag()
    {
        app.set_status(format!("Pomodoro set to {m} min"));
    }
    if resp.double_clicked() {
        let _ = app.run("ui.pomodoro", json!({ "action": "reset" }));
    } else if resp.clicked() {
        let _ = app.run("ui.pomodoro", json!({ "action": "toggle" }));
    }
}

// ---------------- AQC import preview ----------------

/// What `bridge.import` would do with a file, shown until the user applies or cancels.
pub struct BridgePending {
    path: String,
    result: Value,
    overwrite: bool,
    accept_other: bool,
}

/// Read an AQC file and show what would change. Nothing is applied here.
pub fn start_bridge_import(app: &mut CadApp, path: &str) {
    match app.run("bridge.import", json!({ "path": path })) {
        Ok(result) => app.widgets.bridge = Some(BridgePending { path: path.to_string(), result, overwrite: false, accept_other: false }),
        Err(e) => app.set_status(e),
    }
}

fn bridge_window(app: &mut CadApp, ctx: &egui::Context) {
    let t = Tokens::get();
    let mut open = true;
    let mut apply = false;
    let mut rerun = false;
    {
        let Some(b) = app.widgets.bridge.as_mut() else { return };
        egui::Window::new(crate::tl!("Import from AQC"))
            .id(egui::Id::new("bridge_import"))
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(&b.path).small().color(t.text_dim));
                ui.label(b.result["summary"].as_str().unwrap_or(""));
                ui.label(RichText::new(crate::tl!("Geometry is never changed by this import.")).small().color(t.text_dim));
                if b.result["otherDrawing"].as_bool() == Some(true) {
                    ui.checkbox(&mut b.accept_other, crate::tl!("This file came from a different drawing; apply anyway"));
                }
                if ui.checkbox(&mut b.overwrite, crate::tl!("Overwrite fields already filled in AAD")).changed() {
                    rerun = true;
                }
                ui.separator();
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for it in b.result["items"].as_array().into_iter().flatten() {
                        let action = it["action"].as_str().unwrap_or("");
                        let color = if matches!(action, "add" | "update") { t.accent } else { t.text_dim };
                        let to = it["to"].to_string().trim_matches('"').to_string();
                        let reason = it["reason"].as_str().filter(|r| !r.is_empty()).map(|r| format!(" ({r})")).unwrap_or_default();
                        let line = if it["to"].is_null() {
                            format!("{action}  {} · {}{reason}", it["section"].as_str().unwrap_or(""), it["field"].as_str().unwrap_or(""))
                        } else {
                            format!("{action}  {} · {} → {to}{reason}", it["section"].as_str().unwrap_or(""), it["field"].as_str().unwrap_or(""))
                        };
                        ui.label(RichText::new(line).color(color));
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let changes =
                        b.result["items"].as_array().is_some_and(|a| a.iter().any(|i| matches!(i["action"].as_str(), Some("add" | "update"))));
                    let blocked = b.result["otherDrawing"].as_bool() == Some(true) && !b.accept_other;
                    if ui.add_enabled(changes && !blocked, egui::Button::new(crate::tl!("Apply"))).clicked() {
                        apply = true;
                    }
                    if ui.button(crate::tl!("Cancel")).clicked() {
                        // Closed below, like the window's own close button.
                        ui.data_mut(|d| d.insert_temp(egui::Id::new("bridge_cancel"), true));
                    }
                });
            });
    }
    if ctx.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("bridge_cancel"))).unwrap_or(false) {
        open = false;
    }
    if rerun && let Some(b) = app.widgets.bridge.as_ref() {
        let (path, overwrite) = (b.path.clone(), b.overwrite);
        if let Ok(r) = app.run("bridge.import", json!({ "path": path, "overwrite": overwrite }))
            && let Some(b) = app.widgets.bridge.as_mut()
        {
            b.result = r;
        }
    }
    if apply && let Some(b) = app.widgets.bridge.take() {
        match app.run("bridge.import", json!({ "path": b.path, "apply": true, "overwrite": b.overwrite, "acceptOtherDrawing": b.accept_other })) {
            Ok(r) => app.set_status(r["message"].as_str().unwrap_or("Applied from AQC").to_string()),
            Err(e) => app.set_status(e),
        }
        return;
    }
    if !open {
        app.widgets.bridge = None;
    }
}
