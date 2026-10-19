//! Windows shell host.
//!
//! The WinUI app calls the C ABI in [`ffi`]. This crate owns the [`aad_engine::Session`], packs
//! the display list with [`aad_render::mesh`], and on Windows paints that mesh into a child
//! window. It exposes the registry menu, the command line, form descriptions and grip editing.
//!
//! `unsafe` is denied except in the ABI, the child window, and the swap-chain surface.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod assistant;
mod bar;
mod chrome;
mod control;
mod ffi;
mod forms;
#[cfg(windows)]
mod gpu;
mod overlay;
mod panels;
#[cfg(windows)]
mod surface;
#[cfg(windows)]
mod window;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use aad_color::Rgb;
use aad_engine::Session;
use aad_geom::Vec2;
use aad_render::mesh::{self, MeshData};
use serde_json::Value;

/// Wheel notch. Positive zooms in, about the pointer.
pub const POINTER_WHEEL: u32 = 1;
/// Left button pressed. Places a point, or picks when no command is running.
pub const POINTER_LEFT_DOWN: u32 = 2;
/// Left button released. Closes a selection window after a drag.
pub const POINTER_LEFT_UP: u32 = 3;
/// Pointer moved. Pans while the middle button is down, and opens a selection window on a drag.
pub const POINTER_MOVE: u32 = 4;
/// Middle button pressed. The next moves pan the view.
pub const POINTER_MIDDLE_DOWN: u32 = 5;
/// Middle button released.
pub const POINTER_MIDDLE_UP: u32 = 6;
/// Right button. Enter while a command is running.
pub const POINTER_RIGHT: u32 = 7;
/// Left button double-click. Edits text, or enters a layout viewport.
pub const POINTER_LEFT_DOUBLE: u32 = 8;
/// Middle button double-click. Zooms to the drawing extents.
pub const POINTER_MIDDLE_DOUBLE: u32 = 9;
/// Shift is held.
pub const MOD_SHIFT: u32 = 1;
/// Ctrl is held.
pub const MOD_CTRL: u32 = 2;

/// One pointer event. Screen pixels are relative to the drawing surface, origin top-left.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    pub x: f64,
    pub y: f64,
    pub action: u32,
    pub wheel: f32,
    /// [`MOD_SHIFT`] when that key is held. Ignored for the wheel.
    pub mods: u32,
}

/// Counts the WinUI status line reads back.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub entities: u32,
    pub _pad: u32,
    pub view_height: f64,
    pub width: u32,
    pub height: u32,
}

/// Wheel events posted by the child window, applied on the next [`Host::pump`].
#[cfg(windows)]
pub struct Bridge {
    pending: Mutex<Vec<Pointer>>,
}

#[cfg(windows)]
impl Bridge {
    fn new() -> Self {
        Bridge { pending: Mutex::new(Vec::new()) }
    }

    fn push_wheel(&self, x: f64, y: f64, notches: f32) {
        self.push(Pointer { x, y, action: POINTER_WHEEL, wheel: notches, mods: 0 });
    }

    fn push(&self, ev: Pointer) {
        let mut queue = match self.pending.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if ev.action == POINTER_MOVE
            && let Some(last) = queue.last_mut()
            && last.action == POINTER_MOVE
        {
            *last = ev;
            return;
        }
        if queue.len() == 64 {
            queue.remove(0);
        }
        queue.push(ev);
    }

    fn drain(&self) -> Vec<Pointer> {
        let mut queue = match self.pending.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        std::mem::take(&mut *queue)
    }
}

/// The values beside the cursor. The command line already shows the prompt.
/// Kept for the overlay test; the WinUI fields use [`dyn_boxes`].
#[cfg(test)]
fn dyn_line(prompt: &aad_engine::Prompt, frame: Option<&aad_engine::dyninput::Frame>, draft: &str, aim: Vec2) -> String {
    let line = if let Some(frame) = frame
        && let Some(entry) = aad_engine::dyninput::parse(draft, frame)
    {
        let held = aad_engine::dyninput::constrain(&entry, frame, aim);
        let (first_live, second_live) = frame.live(entry.format, held);
        let dist = |v: f64| aad_engine::units::format_distance(v, 2, 4);
        let first = if entry.first.is_empty() { dist(first_live) } else { entry.first.to_string() };
        let second = if entry.second.is_empty() {
            match entry.format {
                aad_engine::dyninput::Format::Cartesian => dist(second_live),
                aad_engine::dyninput::Format::Polar => format!("{:.1}", second_live.to_degrees()),
            }
        } else {
            entry.second.to_string()
        };
        let mark = if frame.relative() { "@ " } else { "" };
        match entry.format {
            aad_engine::dyninput::Format::Cartesian => format!("{mark}{first} , {second}"),
            aad_engine::dyninput::Format::Polar => format!("{mark}{first} < {second}"),
        }
    } else if !draft.is_empty() {
        draft.to_string()
    } else if let Some(base) = prompt.base.filter(|base| base.is_finite() && aim.is_finite()) {
        let deg = (base.angle_to(aim).to_degrees() * 10.0).round() / 10.0;
        format!("{} < {deg}", aad_engine::units::format_distance(base.dist(aim), 2, 4))
    } else if aim.is_finite() {
        format!("{}, {}", aad_engine::units::format_distance(aim.x, 2, 4), aad_engine::units::format_distance(aim.y, 2, 4))
    } else {
        String::new()
    };
    line.chars().take(48).collect()
}

/// The two Dynamic Input boxes. The command-line text stays the source of truth; this is only the view.
fn dyn_boxes(prompt: &aad_engine::Prompt, frame: &aad_engine::dyninput::Frame, draft: &str, aim: Vec2) -> (String, String, String, String, i32) {
    let dist = |v: f64| aad_engine::units::format_distance(v, 2, 4);
    let clip = |text: String| text.chars().take(24).collect();
    // The first point of a command, nothing typed: there is no distance to show, and the cursor's
    // own X and Y are not shown.
    if draft.trim().is_empty() && frame.base.is_none() {
        return (String::new(), String::new(), String::new(), String::new(), 0);
    }
    if let Some(entry) = aad_engine::dyninput::parse(draft, frame) {
        let held = aad_engine::dyninput::constrain(&entry, frame, aim);
        let (first_live, second_live) = frame.live(entry.format, held);
        let first = if entry.first.is_empty() { dist(first_live) } else { entry.first.to_string() };
        let second = if entry.second.is_empty() {
            match entry.format {
                aad_engine::dyninput::Format::Cartesian => dist(second_live),
                aad_engine::dyninput::Format::Polar => format!("{:.1}°", second_live.to_degrees()),
            }
        } else {
            entry.second.to_string()
        };
        let (a_label, b_label) = match entry.format {
            aad_engine::dyninput::Format::Cartesian => ("X", "Y"),
            aad_engine::dyninput::Format::Polar => ("Length", "Angle"),
        };
        let active = if entry.split { 1 } else { 0 };
        return (a_label.into(), b_label.into(), clip(first), clip(second), active);
    }
    if !draft.is_empty() {
        return ("Input".into(), String::new(), clip(draft.to_string()), String::new(), 0);
    }
    if let Some(base) = prompt.base.filter(|base| base.is_finite() && aim.is_finite()) {
        let deg = (base.angle_to(aim).to_degrees() * 10.0).round() / 10.0;
        return ("Length".into(), "Angle".into(), dist(base.dist(aim)), format!("{deg:.1}°"), 0);
    }
    // No live X and Y readout: the cursor shows what is being entered, not where it is.
    (String::new(), String::new(), String::new(), String::new(), 0)
}

/// What is typed while no point is being asked for: the command name or the answer, near the cursor.
fn draft_box(draft: &str, running: bool) -> (String, String, String, String, i32) {
    let label = if running { "Input" } else { "Command" };
    (label.into(), String::new(), draft.chars().take(24).collect(), String::new(), 0)
}

/// One editing session and, on Windows, its drawing surface.
pub struct Host {
    session: Session,
    /// The view the drawing was last fitted to (centre x, y and height). While the view is still
    /// exactly this one nobody has navigated, so a change of the drawing area refits it.
    fitted: Option<(f64, f64, f64)>,
    width: u32,
    height: u32,
    error: String,
    result: String,
    /// Shortcut or snap menu requested by a right-click, as JSON. Empty when none is waiting.
    menu_request: String,
    #[cfg(windows)]
    bridge: Arc<Bridge>,
    #[cfg(windows)]
    hwnd: isize,
    #[cfg(windows)]
    gpu: Option<gpu::Gpu>,
    /// `embed`, `move_child`, and `pump` present on the UI thread. A nested call from inside
    /// `Present` must not touch the surface again.
    #[cfg(windows)]
    in_call: bool,
    /// Command lines that arrived while `in_call` was set. Run after the frame, not dropped.
    #[cfg(windows)]
    pending_submit: Vec<String>,
    /// Screen point where the left or middle button went down.
    press: Option<Vec2>,
    panning: bool,
    dragged: bool,
    dark: bool,
    /// Device pixels per logical point. Hairlines are one point wide.
    pixels_per_point: f32,
    clock: std::time::Instant,
    /// Last pointer, in world units, before snapping.
    raw: Option<Vec2>,
    /// The point a click will use.
    aim: Vec2,
    screen: Option<Vec2>,
    snap: Option<aad_engine::snap::SnapHit>,
    hot: Option<overlay::HotGrip>,
    /// Command-line text that has not been submitted. Dynamic Input reads it.
    draft: String,
    /// Tab cycling: the raw point where it started, and how many times Tab has advanced.
    snap_cycle: Option<(Vec2, usize)>,
    show_ucs: bool,
    canvas_visible: bool,
    assistant: assistant::Desk,
    control_rx: Option<std::sync::mpsc::Receiver<control::Job>>,
    control_port: u16,
    control_stop: Option<Arc<AtomicBool>>,
    /// Set by `app.quit` on the control channel; the shell closes when chrome reports it.
    quit_requested: bool,
    /// A drawing being read on a worker thread: its path and the channel the result arrives on.
    loading: Option<(String, std::sync::mpsc::Receiver<Result<aad_engine::cmd::file::Prepared, String>>)>,
    /// A drawing being written on a worker thread.
    saving: Option<(Arc<aad_engine::cmd::file::SaveJob>, std::sync::mpsc::Receiver<Result<usize, String>>)>,
    /// A plot being built on a worker thread.
    plotting: Option<std::sync::mpsc::Receiver<Result<Value, String>>>,
}

/// DXF and DWG read and write. The engine stays free of file formats; the host installs them
/// once. DWG is converted by a worker process when that program is beside the shell.
fn install_formats() {
    aad_engine::cmd::file::set_io(aad_engine::cmd::file::IoHooks {
        read: |bytes, name| aad_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |drawing, name| aad_io::write(drawing, name).map_err(|e| e.to_string()),
        plot: Some(|drawing, space, opts| aad_io::plot(drawing, space, opts).map_err(|e| e.to_string())),
    });
    if let Some(worker) = dwg_worker() {
        let _ = aad_dwg::isolated::enable_with(worker);
    }
}

fn dwg_worker() -> Option<std::path::PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let names = ["aad-dwg-worker", "aad", "aad-cli"];
    for name in names {
        let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
        for path in [dir.join(&file), dir.join("..").join(&file), dir.join("../debug").join(&file)] {
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

impl Host {
    /// `sample` opens the built-in sample drawing. Otherwise the session starts empty.
    pub fn create(sample: bool) -> Self {
        install_formats();
        let mut session = Session::empty();
        if sample {
            session.open_drawing(aad_engine::sample::default_sample(), "Sample", None);
        } else {
            session.new_drawing(false);
        }
        session.viewport_px = (1200.0, 800.0);
        let _ = session.zoom_extents();
        Host {
            session,
            fitted: None,
            width: 1200,
            height: 800,
            error: String::new(),
            result: String::new(),
            menu_request: String::new(),
            #[cfg(windows)]
            bridge: Arc::new(Bridge::new()),
            #[cfg(windows)]
            hwnd: 0,
            #[cfg(windows)]
            gpu: None,
            #[cfg(windows)]
            in_call: false,
            #[cfg(windows)]
            pending_submit: Vec::new(),
            press: None,
            panning: false,
            dragged: false,
            dark: true,
            pixels_per_point: 1.0,
            clock: std::time::Instant::now(),
            raw: None,
            aim: Vec2::ZERO,
            screen: None,
            snap: None,
            hot: None,
            draft: String::new(),
            snap_cycle: None,
            show_ucs: true,
            canvas_visible: true,
            assistant: assistant::Desk::new(),
            control_rx: None,
            control_port: 0,
            control_stop: None,
            quit_requested: false,
            loading: None,
            saving: None,
            plotting: None,
        }
    }

    /// Start reading a drawing file on a worker thread. The window stays responsive; the shell
    /// calls [`Host::open_poll`] until it reports the result.
    pub fn open_start(&mut self, path: &str) -> Result<(), String> {
        if self.loading.is_some() {
            return Err("a drawing is already being opened".into());
        }
        if path.is_empty() || path.len() > 4096 {
            return Err("missing path".into());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let owned = path.to_string();
        std::thread::Builder::new()
            .name("aad-open".into())
            .spawn(move || {
                let read = std::panic::catch_unwind(|| aad_engine::cmd::file::read_drawing_file(&owned))
                    .unwrap_or_else(|_| Err("internal error while reading the file".into()));
                let _ = tx.send(read);
            })
            .map_err(|e| e.to_string())?;
        self.loading = Some((path.to_string(), rx));
        Ok(())
    }

    /// `Ok(0)` nothing is loading, `Ok(1)` still reading, `Ok(2)` the drawing is open and fitted.
    /// A failed read or import is the error; the drawing that was open is untouched.
    pub fn open_poll(&mut self) -> Result<i32, String> {
        use std::sync::mpsc::TryRecvError;
        let Some((path, rx)) = self.loading.as_ref() else {
            return Ok(0);
        };
        let read = match rx.try_recv() {
            Ok(read) => read,
            Err(TryRecvError::Empty) => return Ok(1),
            Err(TryRecvError::Disconnected) => Err("the file reader stopped".into()),
        };
        let path = path.clone();
        self.loading = None;
        let imported = read.and_then(|d| aad_engine::cmd::file::finish_open(&mut self.session, d, &path).map_err(|e| e.to_string()));
        match imported {
            Ok(_) => {
                self.fit_view();
                Ok(2)
            }
            Err(e) => {
                self.session.echo(e.clone());
                Err(e)
            }
        }
    }

    /// Start writing the active drawing on a worker thread: to `path`, or to its own file when
    /// `None`. The shell calls [`Host::save_poll`] until it reports the result.
    pub fn save_start(&mut self, path: Option<&str>) -> Result<(), String> {
        if self.saving.is_some() {
            return Err("a drawing is already being saved".into());
        }
        if path.is_some_and(|p| p.is_empty() || p.len() > 4096) {
            return Err("missing path".into());
        }
        let job = Arc::new(aad_engine::cmd::file::begin_save(&self.session, path).map_err(|e| e.to_string())?);
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = job.clone();
        std::thread::Builder::new()
            .name("aad-save".into())
            .spawn(move || {
                let written = std::panic::catch_unwind(|| worker.run()).unwrap_or_else(|_| Err("internal error while saving the file".into()));
                let _ = tx.send(written);
            })
            .map_err(|e| e.to_string())?;
        self.saving = Some((job, rx));
        Ok(())
    }

    /// `Ok(0)` nothing is saving, `Ok(1)` still writing, `Ok(2)` saved. A failed save is the error
    /// and the drawing keeps its unsaved changes.
    pub fn save_poll(&mut self) -> Result<i32, String> {
        use std::sync::mpsc::TryRecvError;
        let Some((_, rx)) = self.saving.as_ref() else {
            return Ok(0);
        };
        let written = match rx.try_recv() {
            Ok(written) => written,
            Err(TryRecvError::Empty) => return Ok(1),
            Err(TryRecvError::Disconnected) => Err("the file writer stopped".into()),
        };
        let Some((job, _)) = self.saving.take() else {
            return Ok(0);
        };
        let done = written.and_then(|bytes| aad_engine::cmd::file::finish_save(&mut self.session, &job, bytes).map_err(|e| e.to_string()));
        match done {
            Ok(_) => Ok(2),
            Err(e) => {
                self.session.echo(e.clone());
                Err(e)
            }
        }
    }

    /// Start a plot (`plot` or `exportpdf` with its parameters, `path` included) on a worker
    /// thread. The shell calls [`Host::plot_poll`] until it reports the result.
    pub fn plot_start(&mut self, command: &str, params: &Value) -> Result<(), String> {
        if self.plotting.is_some() {
            return Err("a plot is already running".into());
        }
        if command != "plot" && command != "exportpdf" {
            return Err("unknown plot command".into());
        }
        let job = aad_engine::cmd::layout::begin_plot(&self.session, params, command).map_err(|e| e.to_string())?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("aad-plot".into())
            .spawn(move || {
                let done = std::panic::catch_unwind(|| job.run()).unwrap_or_else(|_| Err("internal error while plotting".into()));
                let _ = tx.send(done);
            })
            .map_err(|e| e.to_string())?;
        self.plotting = Some(rx);
        Ok(())
    }

    /// `Ok(0)` no plot is running, `Ok(1)` still building, `Ok(2)` the file is written. A failure
    /// is the error.
    pub fn plot_poll(&mut self) -> Result<i32, String> {
        use std::sync::mpsc::TryRecvError;
        let Some(rx) = self.plotting.as_ref() else {
            return Ok(0);
        };
        let done = match rx.try_recv() {
            Ok(done) => done,
            Err(TryRecvError::Empty) => return Ok(1),
            Err(TryRecvError::Disconnected) => Err("the plot worker stopped".into()),
        };
        self.plotting = None;
        match done {
            Ok(_) => Ok(2),
            Err(e) => {
                self.session.echo(e.clone());
                Err(e)
            }
        }
    }

    pub fn execute(&mut self, command: &str, params: &Value) -> Result<Value, String> {
        if command.is_empty() || command.len() > 128 {
            return Err("missing command".into());
        }
        self.result.clear();
        let value = match self.session.execute(command, params) {
            Ok(value) => value,
            Err(e) => {
                let message = e.to_string();
                self.session.echo(message.clone());
                return Err(message);
            }
        };
        let text = value.to_string();
        if text.len() > 8 * 1_024 * 1_024 {
            return Err("result is too large".into());
        }
        self.result = text;
        Ok(value)
    }

    /// Run one command-line line, the same path as typing it and pressing Enter.
    pub fn submit(&mut self, text: &str) -> Result<(), String> {
        #[cfg(windows)]
        if self.in_call {
            if self.pending_submit.len() < 8 {
                self.pending_submit.push(text.to_string());
                return Ok(());
            }
            self.session.echo("command was not run".to_string());
            return Err("the drawing is busy".into());
        }
        let result = self.exclusive(|host| host.submit_inner(text));
        #[cfg(windows)]
        self.flush_submit();
        result
    }

    /// Lines queued while a frame was being presented.
    #[cfg(windows)]
    fn flush_submit(&mut self) {
        let mut rounds = 0;
        while rounds < 8 && !self.in_call && !self.pending_submit.is_empty() {
            let pending = std::mem::take(&mut self.pending_submit);
            for text in pending {
                let _ = self.exclusive(|host| host.submit_inner(&text));
            }
            rounds += 1;
        }
    }

    /// Escape. A hot grip is cleared; otherwise the running command is cancelled.
    pub fn cancel_command(&mut self) -> Result<(), String> {
        self.exclusive(|host| {
            host.draft.clear();
            if host.hot.take().is_some() {
                host.session.echo("*Cancel*");
                return host.present_if_ready();
            }
            if host.assistant.busy() {
                host.assistant.stop(&mut host.session);
                return host.present_if_ready();
            }
            host.session.cancel();
            host.present_if_ready()
        })
    }

    pub fn menu_json(&self) -> String {
        chrome::menu_json(&self.session)
    }

    pub fn chrome_json(&self) -> String {
        let cursor = self.screen.filter(|point| point.is_finite()).map(|_| self.aim);
        let mut value: Value = serde_json::from_str(&chrome::chrome_json(&self.session, cursor, &self.draft)).unwrap_or(Value::Null);
        if let Some(object) = value.as_object_mut() {
            object.insert("assistant".into(), Value::String(self.assistant.phase().to_string()));
            object.insert("assistantPrompt".into(), Value::String(self.assistant.prompt().to_string()));
            object.insert("assistantPartial".into(), Value::String(self.assistant.partial().to_string()));
            object.insert("assistantModel".into(), Value::String(self.assistant.model_status().to_string()));
            object.insert("controlPort".into(), Value::from(self.control_port));
            object.insert("quit".into(), Value::from(self.quit_requested));
            object.insert("ucs".into(), Value::from(self.show_ucs));
            object.insert("dyn".into(), self.dyn_chrome());
        }
        value.to_string()
    }

    /// Screen position and the two cursor fields. `on` is false when Dynamic Input is not relevant.
    fn dyn_chrome(&self) -> Value {
        let Some(screen) = self.screen.filter(|point| point.is_finite()) else {
            return serde_json::json!({ "on": false });
        };
        if !self.session.settings.dynmode || self.hot.is_some() || self.session.pending_window.is_some() {
            return serde_json::json!({ "on": false });
        }
        let running = self.session.running.is_some();
        // Assistant requests (a `/` request or a sentence) are written in the command bar only.
        if !running && matches!(bar::classify(&self.draft), bar::Kind::Reference | bar::Kind::Natural) {
            return serde_json::json!({ "on": false });
        }
        let (a_label, b_label, a, b, active) = match (self.session.current_prompt(), self.dyn_frame()) {
            (Some(prompt), Some(frame)) => dyn_boxes(&prompt, &frame, &self.draft, self.aim),
            _ if !self.draft.trim().is_empty() => draft_box(self.draft.trim_start(), running),
            _ => return serde_json::json!({ "on": false }),
        };
        if a.is_empty() && b.is_empty() {
            return serde_json::json!({ "on": false });
        }
        serde_json::json!({
            "on": true,
            "x": screen.x,
            "y": screen.y,
            "aLabel": a_label,
            "a": a,
            "bLabel": b_label,
            "b": b,
            "active": active,
        })
    }

    /// Drop the command-line log only. The drawing, the running command and the registry stay.
    pub fn clear_log(&mut self) -> Result<(), String> {
        self.session.log.clear();
        Ok(())
    }

    /// Properties, layers and blocks, tagged with the drawing id and revision.
    pub fn panels_json(&self) -> String {
        panels::panels_json(&self.session)
    }

    /// Remember what is typed at the command line, so Dynamic Input can follow it.
    pub fn set_draft(&mut self, text: &str) -> Result<(), String> {
        if text.len() > 4_096 {
            return Err("text is too long".into());
        }
        self.draft = text.to_string();
        #[cfg(windows)]
        if self.in_call {
            return Ok(());
        }
        if let Some(screen) = self.screen {
            self.place_cursor(screen.x, screen.y, false)?;
        }
        self.present_if_ready()
    }

    /// Tab at a point prompt: the separator for the format on show (`40` becomes `40<`).
    pub fn dyn_tab(&self, text: &str) -> Option<String> {
        if text.len() > 4_096 {
            return None;
        }
        let frame = self.dyn_frame()?;
        let sep = aad_engine::dyninput::tab_separator(text, &frame)?;
        let mut next = text.to_string();
        next.push(sep);
        Some(next)
    }

    fn submit_inner(&mut self, text: &str) -> Result<(), String> {
        if text.len() > 1_048_576 {
            let message = "text is too long".to_string();
            self.session.echo(message.clone());
            return Err(message);
        }
        if self.assistant.confirming() {
            let word = text.trim().to_ascii_lowercase();
            let accept = matches!(word.as_str(), "y" | "yes" | "allow");
            let decline = matches!(word.as_str(), "n" | "no" | "decline");
            self.draft.clear();
            if accept || decline {
                self.assistant.confirm(&mut self.session, accept);
                return self.present_if_ready();
            }
            let message = "Allow or Decline".to_string();
            self.session.echo(message.clone());
            return Err(message);
        }
        if self.session.running.is_none() && self.hot.is_none() {
            // Assistant requests need a leading `/` (or a `/reference` word). Plain sentences stay
            // on the command line so Space/Enter run CAD commands.
            if matches!(bar::classify(text), bar::Kind::Reference) {
                return self.ask_assistant(text);
            }
        }
        let finished = self.finish_dyn(text);
        self.draft.clear();
        let result = self.session.cmdline(&finished);
        if let Err(e) = &result {
            self.session.echo(e.to_string());
        }
        let result = result.map_err(|e| e.to_string());
        self.present_if_ready()?;
        result
    }

    fn present_if_ready(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        if self.gpu.is_some() {
            return self.redraw();
        }
        Ok(())
    }

    fn exclusive(&mut self, body: impl FnOnce(&mut Self) -> Result<(), String>) -> Result<(), String> {
        #[cfg(windows)]
        if self.in_call {
            return Ok(());
        }
        #[cfg(windows)]
        {
            self.in_call = true;
        }
        let result = body(self);
        #[cfg(windows)]
        {
            self.in_call = false;
        }
        result
    }

    pub fn pointer(&mut self, ev: Pointer) -> Result<(), String> {
        if !ev.x.is_finite() || !ev.y.is_finite() || !ev.wheel.is_finite() {
            return Err("pointer values must be finite".into());
        }
        match ev.action {
            POINTER_WHEEL => self.wheel(ev),
            POINTER_LEFT_DOWN => self.left_down(ev),
            POINTER_LEFT_UP => self.left_up(ev),
            POINTER_MOVE => self.moved(ev),
            POINTER_MIDDLE_DOWN => {
                self.panning = true;
                self.press = Some(Vec2::new(ev.x, ev.y));
                Ok(())
            }
            POINTER_MIDDLE_UP => {
                self.panning = false;
                self.press = None;
                Ok(())
            }
            POINTER_RIGHT => self.right_click(ev),
            POINTER_LEFT_DOUBLE => self.left_double(ev),
            POINTER_MIDDLE_DOUBLE => self.middle_double(),
            _ => Err("unsupported pointer action".into()),
        }
    }

    /// Black drawing area, or white. Colour 7 follows the background.
    /// One-page PDF of the layout on screen, fitted to the sheet.
    pub fn pdf_bytes(&mut self) -> Result<Vec<u8>, String> {
        let mut produced: Option<Result<Vec<u8>, String>> = None;
        self.exclusive(|host| {
            produced = Some(host.plot_pdf());
            Ok(())
        })?;
        produced.unwrap_or_else(|| Err("the preview is busy".into()))
    }

    fn plot_pdf(&self) -> Result<Vec<u8>, String> {
        let state = self.session.state().map_err(|e| e.to_string())?;
        let opts = aad_io::PdfOptions { compress: true, fit: Some(true), ..aad_io::PdfOptions::default() };
        let bytes = aad_io::pdf(&state.doc, &state.space, &opts).map_err(|e| e.to_string())?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("the PDF is too large to preview".into());
        }
        if !bytes.starts_with(b"%PDF") {
            return Err("the plot was not a PDF".into());
        }
        Ok(bytes)
    }

    pub fn set_dark(&mut self, dark: bool) -> Result<(), String> {
        self.exclusive(|host| {
            host.dark = dark;
            host.present_if_ready()
        })
    }

    pub fn set_ucs(&mut self, on: bool) -> Result<(), String> {
        self.exclusive(|host| {
            host.show_ucs = on;
            host.present_if_ready()
        })
    }

    /// Hide the child drawing window so a XAML page can use that cell. `pump` will not raise it
    /// until it is shown again.
    pub fn set_canvas_visible(&mut self, visible: bool) -> Result<(), String> {
        self.canvas_visible = visible;
        #[cfg(windows)]
        if !visible && self.hwnd != 0 {
            window::hide(self.hwnd);
        }
        Ok(())
    }

    /// Listen on `127.0.0.1`. Port `0` picks a free port. A second call returns the port already bound.
    pub fn listen(&mut self, port: u16) -> Result<u16, String> {
        if self.control_port != 0 {
            return Ok(self.control_port);
        }
        let (bound, rx, stop) = control::start(port)?;
        self.control_port = bound;
        self.control_rx = Some(rx);
        self.control_stop = Some(stop);
        Ok(bound)
    }

    pub fn assistant_reply(&mut self, accept: bool) -> Result<(), String> {
        self.exclusive(|host| {
            if !host.assistant.confirming() {
                return Err("the assistant is not waiting".into());
            }
            host.assistant.confirm(&mut host.session, accept);
            host.present_if_ready()
        })
    }

    fn ask_assistant(&mut self, text: &str) -> Result<(), String> {
        self.assistant.ask(&mut self.session, text)?;
        self.present_if_ready()
    }

    fn ucs_hit(&self, x: f64, y: f64) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        let scale = f64::from(self.pixels_per_point.max(1.0));
        let x0 = 16.0 * scale;
        let x1 = 130.0 * scale;
        let y1 = f64::from(self.height) - 8.0 * scale;
        let y0 = y1 - 96.0 * scale;
        x >= x0 && x <= x1 && y >= y0 && y <= y1
    }

    fn drain_control(&mut self) {
        let Some(rx) = &self.control_rx else { return };
        let jobs: Vec<control::Job> = std::iter::from_fn(|| rx.try_recv().ok()).take(32).collect();
        for job in jobs {
            if !job.begin() {
                continue;
            }
            let value = self.control_one(&job.method, &job.params);
            let _ = job.reply.send(value);
        }
    }

    fn control_one(&mut self, method: &str, params: &Value) -> Value {
        let text = |key: &str| params.get(key).and_then(Value::as_str).unwrap_or("");
        match method {
            "engine.execute" | "command" => {
                let Some(id) = params.get("command").and_then(Value::as_str).or_else(|| params.get("id").and_then(Value::as_str)) else {
                    return json_err("missing `command`");
                };
                let body = params.get("params").cloned().unwrap_or(serde_json::json!({}));
                let body = if body.is_null() { serde_json::json!({}) } else { body };
                match self.execute(id, &body) {
                    Ok(value) => json_ok(value),
                    Err(e) => json_err(e),
                }
            }
            "cmdline.input" => {
                let before = self.session.log.len();
                let error = self.submit(text("text")).err();
                json_ok(self.cmdline_state(before, error))
            }
            "cmdline.script" => {
                let before = self.session.log.len();
                match self.session.script(text("text")) {
                    Ok(()) => json_ok(serde_json::json!({"output": log_since(&self.session, before), "state": self.cmdline_state(before, None)})),
                    Err(e) => json_err(e.to_string()),
                }
            }
            "cmdline.key" => {
                let before = self.session.log.len();
                let error = match text("key").to_ascii_lowercase().as_str() {
                    "escape" | "esc" => self.cancel_command().err(),
                    _ => self.session.input(aad_engine::Input::Enter).err().map(|e| e.to_string()),
                };
                if let Some(message) = &error {
                    self.session.echo(message.clone());
                }
                json_ok(self.cmdline_state(before, error))
            }
            "cmdline.state" => json_ok(self.cmdline_state(self.session.log.len(), None)),
            "engine.commands" => {
                let list: Vec<Value> = aad_engine::command_specs()
                    .iter()
                    .map(|spec| serde_json::to_value(spec.info(&self.session)).unwrap_or(Value::Null))
                    .filter(|value| !value.is_null())
                    .collect();
                json_ok(Value::Array(list))
            }
            "drawing.inspect" | "document.inspect" => match self.execute("drawing.inspect", params) {
                Ok(value) => json_ok(value),
                Err(e) => json_err(e),
            },
            "ui.menu.invoke" => {
                let Some(id) = params.get("command").and_then(Value::as_str).or_else(|| params.get("id").and_then(Value::as_str)) else {
                    return json_err("missing `command`");
                };
                let before = self.session.log.len();
                let error = self.submit(id).err();
                json_ok(self.cmdline_state(before, error))
            }
            "ui.menu.list" => self.control_menu_list(),
            "ui.inspect" => self.control_inspect(),
            "ui.pointer" => self.control_pointer(params),
            "ui.screenshot" => self.control_screenshot(params),
            "app.open" => {
                let Some(path) = params.get("path").and_then(Value::as_str) else {
                    return json_err("missing path");
                };
                match self.execute("open", &serde_json::json!({ "path": path })) {
                    Ok(value) => json_ok(value),
                    Err(e) => json_err(e),
                }
            }
            "app.save" => {
                let id = if params.get("path").and_then(Value::as_str).is_some() { "saveas" } else { "qsave" };
                match self.execute(id, params) {
                    Ok(value) => json_ok(value),
                    Err(e) => json_err(e),
                }
            }
            "app.quit" => {
                // Programmatic calls never open dialogs: quit without the unsaved-changes prompt.
                self.quit_requested = true;
                json_ok(Value::Null)
            }
            "ui.render" => self.control_render(params),
            other if other.starts_with("ui.") => {
                json_err(format!("{other} is egui chrome automation; use --egui, or ui.pointer / ui.render / ui.screenshot on WinUI"))
            }
            _ => json_err("unknown method"),
        }
    }

    fn control_menu_list(&self) -> Value {
        match serde_json::from_str::<Value>(&self.menu_json()) {
            Ok(Value::Array(items)) => {
                let menus: Vec<Value> = items
                    .into_iter()
                    .map(|node| menu_node_for_control(&node))
                    .collect();
                json_ok(Value::Array(menus))
            }
            Ok(_) | Err(_) => json_err("menu tree is unavailable"),
        }
    }

    fn control_inspect(&self) -> Value {
        let view = self.session.state().map(|state| state.view()).unwrap_or(aad_engine::View { center: Vec2::ZERO, height: 1.0 });
        let settings = &self.session.settings;
        json_ok(serde_json::json!({
            "shell": "winui",
            "canvas": { "width": self.width, "height": self.height, "visible": self.canvas_visible },
            "view": { "center": [view.center.x, view.center.y], "height": view.height },
            "cursor": [self.aim.x, self.aim.y],
            "dark": self.dark,
            "ucs": self.show_ucs,
            "osnap": settings.osmode != 0,
            "ortho": settings.orthomode,
            "polar": settings.polarmode,
            "dynmode": settings.dynmode,
            "controlPort": self.control_port,
            "assistant": self.assistant.phase().to_string(),
            "closePrompt": Value::Null,
            "focus": Value::Null,
        }))
    }

    fn control_pointer(&mut self, params: &Value) -> Value {
        let Some(x) = params.get("x").and_then(Value::as_f64) else {
            return json_err("missing x/y");
        };
        let Some(y) = params.get("y").and_then(Value::as_f64) else {
            return json_err("missing x/y");
        };
        if !x.is_finite() || !y.is_finite() {
            return json_err("x/y must be finite");
        }
        let world = if params.get("space").and_then(Value::as_str).unwrap_or("world") == "screen" {
            match self.screen_to_world(x, y) {
                Ok(point) => point,
                Err(e) => return json_err(e),
            }
        } else {
            Vec2::new(x, y)
        };
        self.aim = world;
        self.session.cursor = world;
        self.session.cursor_deferred = None;
        let action = params.get("action").and_then(Value::as_str).unwrap_or("click");
        if action == "move" {
            return json_ok(serde_json::json!({ "world": [world.x, world.y] }));
        }
        let shift = params.get("shift").and_then(Value::as_bool).unwrap_or(false);
        let before = self.session.log.len();
        let result = match params.get("button").and_then(Value::as_str).unwrap_or("left") {
            "right" => self.session.input(aad_engine::Input::Enter),
            _ if self.session.running.is_some() => self.session.input(aad_engine::Input::Point(world)),
            _ => self.session.idle_click(world, shift),
        };
        match result {
            Ok(()) => {
                let _ = self.present_if_ready();
                json_ok(serde_json::json!({
                    "world": [world.x, world.y],
                    "state": self.cmdline_state(before, None),
                }))
            }
            Err(e) => json_err(e.to_string()),
        }
    }

    /// Drawing PNG (not WinUI chrome). Same raster path as `ui.render`; default path is the temp dir.
    fn control_screenshot(&self, params: &Value) -> Value {
        let path = params
            .get("path")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| std::env::temp_dir().join("aad-shot.png").to_string_lossy().into_owned());
        let mut body = params.clone();
        if let Some(object) = body.as_object_mut() {
            object.insert("path".into(), Value::String(path.clone()));
            object.entry("width".to_string()).or_insert_with(|| Value::from(self.width.max(16)));
            object.entry("height".to_string()).or_insert_with(|| Value::from(self.height.max(16)));
            object.entry("fit".to_string()).or_insert(Value::Bool(false));
        }
        let rendered = self.control_render(&body);
        if rendered.get("ok") != Some(&Value::Bool(true)) {
            return rendered;
        }
        let result = rendered.get("result").cloned().unwrap_or(Value::Null);
        json_ok(serde_json::json!({
            "path": result.get("path").cloned().unwrap_or(Value::String(path)),
            "width": result.get("width").cloned().unwrap_or(Value::from(self.width)),
            "height": result.get("height").cloned().unwrap_or(Value::from(self.height)),
            "drawing": true,
        }))
    }

    /// Headless PNG of the drawing (same contract as egui `ui.render`). No window chrome.
    fn control_render(&self, params: &Value) -> Value {
        let width = params.get("width").and_then(Value::as_f64).unwrap_or(1600.0).clamp(16.0, 8192.0) as u32;
        let height = params.get("height").and_then(Value::as_f64).unwrap_or(1000.0).clamp(16.0, 8192.0) as u32;
        let fit = params.get("fit").and_then(Value::as_bool).unwrap_or(true);
        let Ok(state) = self.session.state() else {
            return json_err("no drawing");
        };
        let list = aad_render::build(&state.doc, &state.space, &aad_render::Options::default());
        let view = state.view();
        let raster = if fit {
            aad_render::raster::View::fit(&list.bounds, width, height, 0.05)
        } else {
            aad_render::raster::View {
                center: view.center,
                scale: f64::from(height) / view.height.max(1e-12),
                width,
                height,
            }
        };
        let Some(png) = aad_render::raster::render_png(&list, &raster, &aad_render::raster::RasterOptions::default()) else {
            return json_err("render failed");
        };
        if let Some(path) = params.get("path").and_then(Value::as_str) {
            if let Err(e) = std::fs::write(path, &png) {
                return json_err(e.to_string());
            }
            return json_ok(serde_json::json!({
                "pngBase64": aad_engine::cmd::file::base64_encode(&png),
                "width": width,
                "height": height,
                "bytes": png.len(),
                "path": path,
            }));
        }
        json_ok(serde_json::json!({
            "pngBase64": aad_engine::cmd::file::base64_encode(&png),
            "width": width,
            "height": height,
            "bytes": png.len(),
        }))
    }

    fn cmdline_state(&self, before: usize, error: Option<String>) -> Value {
        let prompt = self.session.current_prompt();
        serde_json::json!({
            "prompt": self.session.prompt_text(),
            "running": self.session.running.as_ref().map(|running| running.id.clone()),
            "keywords": prompt.as_ref().map(|prompt| prompt.keywords.clone()).unwrap_or_default(),
            "accept": prompt.as_ref().map(|prompt| serde_json::to_value(prompt.accept).unwrap_or(Value::Null)).unwrap_or(Value::Null),
            "buffer": self.draft,
            "history": self.session.log.iter().rev().take(20).collect::<Vec<_>>().into_iter().rev().cloned().collect::<Vec<_>>(),
            "output": log_since(&self.session, before),
            "error": error,
        })
    }

    fn wheel(&mut self, ev: Pointer) -> Result<(), String> {
        if ev.wheel == 0.0 {
            return Ok(());
        }
        let about = self.screen_to_world(ev.x, ev.y)?;
        // One notch is 120 wheel units in the egui shell (`scroll.y / 300`).
        let factor = (f64::from(ev.wheel) * 120.0 / 300.0).exp();
        self.session.zoom_about(factor, about).map_err(|e| e.to_string())
    }

    fn left_down(&mut self, ev: Pointer) -> Result<(), String> {
        self.place_cursor(ev.x, ev.y, ev.mods & MOD_SHIFT != 0)?;
        let shift = ev.mods & MOD_SHIFT != 0;
        if self.session.running.is_some() {
            self.hot = None;
            self.dragged = false;
            self.press = Some(Vec2::new(ev.x, ev.y));
            let input = match self.snap.filter(|hit| hit.deferred.is_some()) {
                Some(hit) => hit.input(),
                None => aad_engine::Input::Point(self.aim),
            };
            return self.session.input(input).map_err(|e| e.to_string());
        }
        if let Some(grip) = self.hot.take() {
            return self.session.grip_edit(grip.handle, grip.index, self.aim, grip.mode).map_err(|e| e.to_string());
        }
        if let Some(grip) = self.grip_at(ev.x, ev.y) {
            self.session.echo(grip.label().to_string());
            self.hot = Some(grip);
            return Ok(());
        }
        if self.show_ucs && self.ucs_hit(ev.x, ev.y) {
            let (x, y) = self.screen_point(ev.x, ev.y);
            self.menu_request = chrome::ucs_popup(x, y);
            return Ok(());
        }
        self.dragged = false;
        self.press = Some(Vec2::new(ev.x, ev.y));
        let world = self.raw.unwrap_or(self.aim);
        self.session.idle_click(world, shift).map_err(|e| e.to_string())
    }

    fn left_up(&mut self, ev: Pointer) -> Result<(), String> {
        let dragged = self.dragged;
        self.press = None;
        self.dragged = false;
        if !dragged || self.session.pending_window.is_none() {
            return Ok(());
        }
        let world = self.screen_to_world(ev.x, ev.y)?;
        let shift = ev.mods & MOD_SHIFT != 0;
        if self.session.running.is_some() {
            self.session.input(aad_engine::Input::Point(world)).map_err(|e| e.to_string())
        } else {
            self.session.idle_click(world, shift).map_err(|e| e.to_string())
        }
    }

    fn moved(&mut self, ev: Pointer) -> Result<(), String> {
        self.place_cursor(ev.x, ev.y, ev.mods & MOD_SHIFT != 0)?;
        if self.hot.is_some() {
            return Ok(());
        }
        if self.panning {
            let Some(last) = self.press else { return Ok(()) };
            self.press = Some(Vec2::new(ev.x, ev.y));
            return self.pan_by(ev.x - last.x, ev.y - last.y);
        }
        let Some(origin) = self.press else { return Ok(()) };
        let dx = ev.x - origin.x;
        let dy = ev.y - origin.y;
        if !self.dragged && dx * dx + dy * dy > 16.0 {
            self.dragged = true;
            if self.session.pending_window.is_none() {
                let world = self.screen_to_world(origin.x, origin.y)?;
                self.session.begin_window(world);
            }
        }
        Ok(())
    }

    fn left_double(&mut self, ev: Pointer) -> Result<(), String> {
        if self.session.running.is_some() {
            return Ok(());
        }
        let (on_paper, paper, inside, had_viewport) = {
            let state = self.session.state().map_err(|e| e.to_string())?;
            let on_paper = matches!(state.space, aad_engine::doc::Space::Paper(_));
            let paper = self.screen_on(ev.x, ev.y, state.paper_view());
            let viewport = state.active_viewport();
            let inside = viewport
                .as_ref()
                .is_some_and(|(_, view)| (paper.x - view.center.x).abs() <= view.width / 2.0 && (paper.y - view.center.y).abs() <= view.height / 2.0);
            (on_paper, paper, inside, viewport.is_some())
        };
        if on_paper && !inside {
            self.hot = None;
            let at = serde_json::json!({ "at": [paper.x, paper.y] });
            if self.session.execute("mspace", &at).is_err() && had_viewport {
                let _ = self.session.execute("pspace", &serde_json::json!({}));
            }
            return Ok(());
        }
        let world = self.screen_to_world(ev.x, ev.y)?;
        self.edit_text_at(world)
    }

    fn edit_text_at(&mut self, world: Vec2) -> Result<(), String> {
        let editable = self.session.state().ok().and_then(|state| match state.selection.as_slice() {
            [handle] => state.doc.entity(*handle).map(|entity| {
                matches!(
                    entity.kind,
                    aad_engine::doc::EntityKind::Text(_)
                        | aad_engine::doc::EntityKind::MText(_)
                        | aad_engine::doc::EntityKind::AttDef(_)
                        | aad_engine::doc::EntityKind::Dimension(_)
                )
            }),
            _ => None,
        });
        if editable != Some(true) {
            return Ok(());
        }
        self.session.start("textedit").map_err(|e| e.to_string())?;
        self.session.input(aad_engine::Input::Point(world)).map_err(|e| e.to_string())
    }

    fn middle_double(&mut self) -> Result<(), String> {
        self.panning = false;
        self.press = None;
        self.session.zoom_extents().map_err(|e| e.to_string())
    }

    fn screen_on(&self, x: f64, y: f64, view: aad_engine::View) -> Vec2 {
        let height = f64::from(self.height.max(1));
        let scale = height / view.height.max(1e-12);
        let cx = f64::from(self.width.max(1)) * 0.5;
        let cy = height * 0.5;
        Vec2::new(view.center.x + (x - cx) / scale, view.center.y - (y - cy) / scale)
    }

    fn right_click(&mut self, ev: Pointer) -> Result<(), String> {
        let snap = ev.mods & (MOD_SHIFT | MOD_CTRL) != 0;
        if self.session.running.is_some() && !snap {
            self.menu_request.clear();
            return self.session.input(aad_engine::Input::Enter).map_err(|e| e.to_string());
        }
        let (x, y) = self.screen_point(ev.x, ev.y);
        self.menu_request = chrome::popup_json(&self.session, snap, x, y);
        Ok(())
    }

    fn screen_point(&self, x: f64, y: f64) -> (i32, i32) {
        #[cfg(windows)]
        {
            window::client_to_screen(self.hwnd, x, y)
        }
        #[cfg(not(windows))]
        {
            let _ = self;
            (i32::try_from(x as i64).unwrap_or(0), i32::try_from(y as i64).unwrap_or(0))
        }
    }

    /// The pending right-click menu, then none.
    pub fn take_popup(&mut self) -> String {
        std::mem::take(&mut self.menu_request)
    }

    /// JSON for a finished `plot.window` request. Empty when nothing is waiting.
    /// `{"window":null}` means extents/display; `{"window":[x0,y0,x1,y1]}` is a rectangle.
    pub fn plot_request_json(&self) -> String {
        match &self.session.plot_request {
            None => String::new(),
            Some(req) => match req.window {
                Some(window) => serde_json::json!({ "window": window }).to_string(),
                None => serde_json::json!({ "window": null }).to_string(),
            },
        }
    }

    /// Clear a finished `plot.window` request after the shell has copied it.
    pub fn clear_plot_request(&mut self) {
        self.session.plot_request = None;
    }

    /// The drawing clipboard as DXF text, or empty when there is nothing to put on the system clipboard.
    pub fn clipboard_text(&self) -> String {
        aad_engine::cmd::clipboard::system_text(&self.session).unwrap_or_default()
    }

    /// Replace the drawing clipboard with DXF text from the system clipboard.
    pub fn load_clipboard(&mut self, text: &str) -> bool {
        if text.len() > 8 * 1_024 * 1_024 {
            return false;
        }
        aad_engine::cmd::clipboard::load_system_text(&mut self.session, text)
    }

    fn pan_by(&mut self, dx: f64, dy: f64) -> Result<(), String> {
        let state = self.session.state_mut().map_err(|e| e.to_string())?;
        let view = state.view();
        let scale = f64::from(self.height.max(1)) / view.height.max(1e-12);
        state.set_view_quiet(aad_engine::View { center: view.center - Vec2::new(dx / scale, -dy / scale), height: view.height });
        Ok(())
    }

    pub fn stats(&self) -> Stats {
        let entities = self.session.doc().map(|d| d.entity_count()).unwrap_or(0);
        let view_height = self.session.state().map(|s| s.view().height).unwrap_or(0.0);
        Stats { entities: u32::try_from(entities).unwrap_or(u32::MAX), _pad: 0, view_height, width: self.width, height: self.height }
    }

    fn place_cursor(&mut self, x: f64, y: f64, shift: bool) -> Result<(), String> {
        let raw = self.screen_to_world(x, y)?;
        self.raw = Some(raw);
        self.screen = Some(Vec2::new(x, y));
        let prompt = self.session.current_prompt();
        let wants = self.hot.is_some() || prompt.as_ref().is_some_and(|p| p.accept.point && !p.accept.select);
        if !wants || self.session.pending_window.is_some() {
            if !wants {
                self.session.tracking.clear();
            }
            self.aim = raw;
            self.snap = None;
            self.session.cursor = raw;
            self.session.cursor_deferred = None;
            return Ok(());
        }
        let scale = self.pixels_per_unit();
        let base = self.hot.map(|grip| grip.base).or_else(|| prompt.as_ref().and_then(|p| p.base));
        let deferred = self.hot.is_none() && prompt.as_ref().is_some_and(|p| p.deferred);
        let query = aad_engine::snap::tracking::CursorQuery {
            base,
            deferred,
            aperture: self.session.settings.aperture / scale,
            tol: 6.0 / scale,
            now: self.clock.elapsed().as_secs_f64(),
            shift,
        };
        let snapped = self.session.snap_cursor(raw, &query);
        self.aim = snapped.point;
        self.snap = snapped.snap;
        self.apply_snap_cycle(raw, self.session.settings.aperture / scale);
        if let Some(frame) = self.dyn_frame()
            && let Some(entry) = aad_engine::dyninput::parse(&self.draft, &frame)
        {
            self.aim = aad_engine::dyninput::constrain(&entry, &frame, self.aim);
        }
        self.session.cursor = self.aim;
        self.session.cursor_deferred = self.snap.and_then(|hit| hit.deferred);
        Ok(())
    }

    /// Enter or Space while a grip is hot. An empty line is left for [`Self::grip_cycle`].
    /// A point (`x,y`, `@dx,dy`, `@d<a`) applies the grip and clears it. Anything else is reported
    /// and still consumed, so it is not run as a command.
    pub fn grip_point(&mut self, text: &str) -> bool {
        if self.hot.is_none() {
            return false;
        }
        if text.len() > 1_048_576 {
            self.session.echo("text is too long".to_string());
            let _ = self.present_if_ready();
            return true;
        }
        let typed = text.trim();
        if typed.is_empty() {
            return false;
        }
        let Some(grip) = self.hot else { return false };
        match aad_engine::prompt::parse_point_with(typed, grip.base, &self.session.angle_settings()) {
            Some(point) => {
                self.hot = None;
                if let Err(error) = self.session.grip_edit(grip.handle, grip.index, point, grip.mode) {
                    self.session.echo(error.to_string());
                }
            }
            None => self.session.echo("Requires a point (x,y, @dx,dy or @d<a)."),
        }
        let _ = self.present_if_ready();
        true
    }

    /// Space or Enter with nothing typed: the hot grip's next mode (stretch, move, rotate, scale, mirror).
    pub fn grip_cycle(&mut self) -> bool {
        let label = {
            let Some(grip) = self.hot.as_mut() else { return false };
            grip.next_mode();
            grip.label().to_string()
        };
        self.session.echo(label);
        let _ = self.present_if_ready();
        true
    }

    /// Move the drafting cursor by arrow keys. `dx`/`dy` are steps on screen (positive `dy` is down).
    /// Ctrl pans the view instead. Shift uses a larger step.
    pub fn key_nudge(&mut self, dx: i32, dy: i32, ctrl: bool, shift: bool) -> Result<(), String> {
        self.exclusive(|host| {
            let dx = dx.clamp(-64, 64);
            let dy = dy.clamp(-64, 64);
            let step = if shift { 40.0 } else { 8.0 };
            let sx = f64::from(dx) * step;
            let sy = f64::from(dy) * step;
            if ctrl {
                host.pan_by(sx, sy)?;
                if let Some(screen) = host.screen {
                    host.place_cursor(screen.x, screen.y, false)?;
                }
                return host.present_if_ready();
            }
            let screen = host.screen.unwrap_or_else(|| {
                Vec2::new(f64::from(host.width.max(1)) * 0.5, f64::from(host.height.max(1)) * 0.5)
            });
            let max_x = f64::from(host.width.max(1));
            let max_y = f64::from(host.height.max(1));
            let x = (screen.x + sx).clamp(0.0, max_x);
            let y = (screen.y + sy).clamp(0.0, max_y);
            host.place_cursor(x, y, shift)?;
            host.present_if_ready()
        })
    }

    /// Place a point, pick or grip, or close a selection window at the keyboard cursor (Insert).
    pub fn key_activate(&mut self, shift: bool) -> Result<(), String> {
        self.exclusive(|host| {
            let screen = match host.screen {
                Some(point) if point.is_finite() => point,
                _ => host.world_to_screen(host.aim).unwrap_or_else(|_| {
                    Vec2::new(f64::from(host.width.max(1)) * 0.5, f64::from(host.height.max(1)) * 0.5)
                }),
            };
            if !screen.x.is_finite() || !screen.y.is_finite() {
                return Err("cursor is not on the drawing".into());
            }
            let mods = if shift { MOD_SHIFT } else { 0 };
            host.left_down(Pointer { x: screen.x, y: screen.y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods })?;
            host.present_if_ready()
        })
    }

    /// Tab with nothing for Dynamic Input to split: the next object snap under the cursor.
    pub fn snap_tab(&mut self) -> Result<(), String> {
        let Some(raw) = self.raw else { return Ok(()) };
        let aperture = self.session.settings.aperture / self.pixels_per_unit();
        let (anchor, index) = self.snap_cycle.unwrap_or((raw, 0));
        let anchor = if anchor.dist(raw) > aperture { raw } else { anchor };
        self.snap_cycle = Some((anchor, index.saturating_add(1)));
        if let Some(screen) = self.screen {
            self.place_cursor(screen.x, screen.y, false)?;
        }
        self.present_if_ready()
    }

    fn apply_snap_cycle(&mut self, raw: Vec2, aperture: f64) {
        let Some((anchor, index)) = self.snap_cycle else { return };
        if !aperture.is_finite() || anchor.dist(raw) > aperture {
            self.snap_cycle = None;
            return;
        }
        let Some(state) = self.session.state().ok() else { return };
        let prompt = self.session.current_prompt();
        let candidates = aad_engine::pointmod::snap_candidates(
            &state.doc,
            &state.edit_space(),
            raw,
            aperture,
            self.session.settings.osmode,
            prompt.as_ref().and_then(|prompt| prompt.base),
            prompt.as_ref().is_some_and(|prompt| prompt.deferred),
        );
        let Some(hit) = candidates.get(index % candidates.len().max(1)).copied() else { return };
        self.aim = hit.point;
        self.snap = Some(hit);
    }

    /// The dialog description for `id`, when the command has settings to ask before any points.
    pub fn form_json(&self, id: &str) -> Option<String> {
        forms::form_json(&self.session, id)
    }

    /// Start `id` with the dialog's answers. Points still missing are picked on the drawing.
    pub fn start_form(&mut self, id: &str, preset: &Value) -> Result<(), String> {
        self.session.start_form(id, preset.clone()).map_err(|e| e.to_string())?;
        self.draft.clear();
        self.present_if_ready()
    }

    /// The Dynamic Input frame, when the value boxes are live.
    fn dyn_frame(&self) -> Option<aad_engine::dyninput::Frame> {
        if !self.session.settings.dynmode || self.hot.is_some() || self.session.pending_window.is_some() {
            return None;
        }
        let prompt = self.session.current_prompt().filter(|prompt| prompt.accept.point && !prompt.accept.select)?;
        Some(aad_engine::dyninput::Frame {
            base: prompt.base,
            cartesian: self.session.settings.dynpi_cartesian,
            absolute: self.session.settings.dynpi_absolute,
        })
    }

    /// A split entry (`40,30`, `40<90`) is completed from the cursor before it is submitted.
    fn finish_dyn(&self, text: &str) -> String {
        let Some(frame) = self.dyn_frame() else { return text.to_string() };
        let drawn = self.session.doc().ok().and_then(aad_engine::annot::drawing_unit);
        aad_engine::units::with_drawn_unit(drawn, || {
            aad_engine::dyninput::parse(text, &frame)
                .and_then(|entry| aad_engine::dyninput::complete(&entry, &frame, self.aim, self.session.last_point))
                .unwrap_or_else(|| text.to_string())
        })
    }

    /// The line drawn beside the cursor while a command is running.
    /// The WinUI shell draws these fields itself; this string is what the overlay test strokes.
    #[cfg(test)]
    fn dyn_tip(&self) -> Option<String> {
        if self.screen.is_none() || self.session.pending_window.is_some() {
            return None;
        }
        if let Some(hot) = self.hot {
            return Some(hot.label().to_string());
        }
        if !self.session.settings.dynmode {
            return None;
        }
        let prompt = self.session.current_prompt()?;
        Some(dyn_line(&prompt, self.dyn_frame().as_ref(), &self.draft, self.aim))
    }

    fn grip_at(&self, x: f64, y: f64) -> Option<overlay::HotGrip> {
        if self.session.running.is_some() {
            return None;
        }
        let state = self.session.state().ok()?;
        let reach = self.session.settings.gripsize.max(1.0) + 3.0;
        for handle in self.session.selection().iter().take(200) {
            let Some(entity) = state.doc.entity(*handle) else { continue };
            if state.doc.layer(&entity.common.layer).is_some_and(|layer| layer.locked) {
                continue;
            }
            for (index, grip) in entity.kind.grips().into_iter().enumerate() {
                let Ok(screen) = self.world_to_screen(grip) else { continue };
                if (screen.x - x).abs() <= reach && (screen.y - y).abs() <= reach {
                    return Some(overlay::HotGrip { handle: *handle, index, base: grip, mode: aad_engine::grips::GripMode::Stretch });
                }
            }
        }
        None
    }

    fn pixels_per_unit(&self) -> f64 {
        let height = self.session.state().map(|state| state.view().height).unwrap_or(1.0);
        f64::from(self.height.max(1)) / height.max(1e-12)
    }

    fn world_to_screen(&self, point: Vec2) -> Result<Vec2, String> {
        let view = self.session.state().map_err(|e| e.to_string())?.view();
        let scale = self.pixels_per_unit();
        let cx = f64::from(self.width.max(1)) * 0.5;
        let cy = f64::from(self.height.max(1)) * 0.5;
        Ok(Vec2::new(cx + (point.x - view.center.x) * scale, cy - (point.y - view.center.y) * scale))
    }

    fn paper(&self) -> Rgb {
        if self.dark { Rgb(0, 0, 0) } else { Rgb(255, 255, 255) }
    }

    fn screen_to_world(&self, x: f64, y: f64) -> Result<Vec2, String> {
        let view = self.session.state().map_err(|e| e.to_string())?.view();
        let height = f64::from(self.height.max(1));
        let scale = height / view.height.max(1e-12);
        let cx = f64::from(self.width.max(1)) * 0.5;
        let cy = height * 0.5;
        Ok(Vec2::new(view.center.x + (x - cx) / scale, view.center.y - (y - cy) / scale))
    }

    fn mesh(&self) -> Result<(u64, MeshData, [f32; 12], Vec2), String> {
        let lw = self.session.settings.lwdisplay;
        let state = self.session.state().map_err(|e| e.to_string())?;
        let view = state.view();
        let px = view.height / f64::from(self.height.max(1));
        let band = px.max(1e-300).log2().floor() as i32;
        let opts = aad_render::Options {
            tolerance: 2f64.powi(band) * 0.5,
            min_dash: px * 2.0,
            text: true,
            fill: true,
            lineweights: lw,
            view_height: view.height,
            plot_style_table: None,
        };
        let list = aad_render::build(&state.doc, &state.space, &opts);
        let origin = mesh::choose_origin(&list.bounds, view.center, view.height);
        let paper = self.paper();
        let mut built = mesh::build_mesh(&list, origin, paper, lw);
        let world_width = view.height * f64::from(self.width.max(1)) / f64::from(self.height.max(1));
        mesh::append_infinite(&mut built, &list, origin, view.center, world_width, view.height, paper);
        mesh::expand_hairlines(&mut built);
        let scale = f64::from(self.height.max(1)) / view.height.max(1e-12);
        let uniform = [
            (origin.x - view.center.x) as f32,
            (origin.y - view.center.y) as f32,
            scale as f32,
            0.0,
            f64::from(self.width.max(1)) as f32 * 0.5,
            f64::from(self.height.max(1)) as f32 * 0.5,
            f64::from(self.width.max(1)) as f32,
            f64::from(self.height.max(1)) as f32,
            self.pixels_per_point,
            0.0,
            0.0,
            0.0,
        ];
        // The sheet colour is part of the key: colour 7 is white on black and black on white, and
        // the GPU keeps the last upload until this key changes.
        let tint = u64::from(paper.0) << 16 | u64::from(paper.1) << 8 | u64::from(paper.2);
        let key = state.revision ^ origin.x.to_bits() ^ origin.y.to_bits().rotate_left(1) ^ tint.rotate_left(21);
        Ok((key, built, uniform, origin))
    }

    /// Vertices in the current frame. Used by tests; the window paints the same mesh.
    pub fn mesh_vertices(&self) -> Result<usize, String> {
        let (_, built, _, _) = self.mesh()?;
        Ok(built.line_vertices() + built.tri_vertices() + built.quad_instances())
    }
}

#[cfg(windows)]
impl Host {
    /// Create the child drawing window inside `parent` and fit the sample to it.
    pub fn embed(&mut self, parent: isize, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
        if self.in_call {
            return Ok(());
        }
        self.in_call = true;
        let result = self.embed_inner(parent, x, y, width, height);
        self.in_call = false;
        result
    }

    fn embed_inner(&mut self, parent: isize, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
        if self.hwnd != 0 {
            return self.move_child_inner(x, y, width, height);
        }
        let hwnd = window::create(parent, x, y, width, height, Arc::as_ptr(&self.bridge))?;
        self.hwnd = hwnd;
        self.apply_size(width, height);
        self.fit_view();
        match gpu::Gpu::open(hwnd, self.width, self.height) {
            Ok(gpu) => self.gpu = Some(gpu),
            Err(e) => {
                window::destroy(hwnd);
                self.hwnd = 0;
                return Err(e);
            }
        }
        self.redraw()
    }

    pub fn move_child(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
        if self.in_call {
            return Ok(());
        }
        self.in_call = true;
        let result = self.move_child_inner(x, y, width, height);
        self.in_call = false;
        result
    }

    fn move_child_inner(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
        if self.hwnd == 0 {
            return Err("no drawing window".into());
        }
        window::move_to(self.hwnd, x, y, width, height)?;
        self.apply_size(width, height);
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.resize(self.width, self.height)?;
        }
        self.redraw()
    }

    /// Apply a queued wheel event and present a frame.
    pub fn pump(&mut self) -> Result<(), String> {
        if self.in_call {
            return Ok(());
        }
        self.drain_control();
        self.assistant.poll(&mut self.session);
        self.in_call = true;
        let result = (|| {
            if self.canvas_visible {
                window::raise(self.hwnd);
            }
            for ev in self.bridge.drain() {
                self.pointer(ev)?;
            }
            if self.gpu.is_some() {
                self.redraw()?;
            }
            Ok(())
        })();
        self.in_call = false;
        self.flush_submit();
        result
    }

    pub fn redraw(&mut self) -> Result<(), String> {
        self.pixels_per_point = window::dpi_scale(self.hwnd);
        let (key, built, uniform, origin) = self.mesh()?;
        let paper = self.paper();
        let cursor =
            self.screen.is_some().then_some(overlay::Cursor { aim: self.aim, raw: self.raw, snap: self.snap.map(|hit| hit.point), hot: self.hot });
        let tip = self.hot.map(|hot| hot.label().to_string());
        let overlay = overlay::mesh(
            &self.session,
            cursor.as_ref(),
            paper,
            self.height,
            origin,
            tip.as_deref(),
            self.width,
            self.show_ucs,
            f64::from(self.pixels_per_point),
        );
        let gpu = self.gpu.as_mut().ok_or("no drawing surface")?;
        gpu.set_clear(paper);
        gpu.set_overlay(&overlay);
        gpu.present(key, &built, &uniform)
    }

    /// Zoom to the extents and remember the view that gave, so a later change of the drawing area
    /// (the window settling, a panel resized) refits it while nobody has navigated.
    fn fit_view(&mut self) {
        let _ = self.session.zoom_extents();
        self.fitted = self.session.state().ok().map(|s| {
            let v = s.view();
            (v.center.x, v.center.y, v.height)
        });
    }

    fn apply_size(&mut self, width: u32, height: u32) {
        let before = (self.width, self.height);
        self.width = width.clamp(1, 8192);
        self.height = height.clamp(1, 8192);
        self.session.viewport_px = (f64::from(self.width), f64::from(self.height));
        // Nobody has navigated since the drawing was fitted: fit it to the new drawing area.
        let untouched = self.session.state().ok().map(|s| s.view()).zip(self.fitted).is_some_and(|(v, f)| (v.center.x, v.center.y, v.height) == f);
        if untouched && (self.width, self.height) != before {
            self.fit_view();
        }
    }
}

#[cfg(windows)]
impl Drop for Host {
    fn drop(&mut self) {
        if let Some(stop) = &self.control_stop {
            stop.store(true, Ordering::Relaxed);
        }
        self.gpu.take();
        if self.hwnd != 0 {
            window::destroy(self.hwnd);
            self.hwnd = 0;
        }
    }
}

fn log_since(session: &Session, before: usize) -> Vec<String> {
    session.log.iter().skip(before).cloned().collect()
}

/// Shape the host menu tree like egui `ui.menu.list` (`label` / `children` / optional `id`).
fn menu_node_for_control(node: &Value) -> Value {
    let label = node.get("title").and_then(Value::as_str).unwrap_or("");
    let children: Vec<Value> = node
        .get("items")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(menu_node_for_control).collect())
        .unwrap_or_default();
    let mut out = serde_json::json!({ "label": label, "children": children });
    if let (Some(id), Some(object)) = (node.get("id").and_then(Value::as_str), out.as_object_mut()) {
        if !id.is_empty() {
            object.insert("id".into(), Value::String(id.to_string()));
        }
    }
    if let (Some(enabled), Some(object)) = (node.get("enabled"), out.as_object_mut()) {
        object.insert("enabled".into(), enabled.clone());
    }
    out
}

fn json_ok(value: Value) -> Value {
    serde_json::json!({"ok": true, "result": value})
}

fn json_err(error: impl std::fmt::Display) -> Value {
    serde_json::json!({"ok": false, "error": error.to_string()})
}

#[cfg(test)]
#[allow(clippy::unwrap_used, unsafe_code)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_resize_refits_the_drawing_until_the_user_navigates() {
        let mut host = Host::create(true);
        host.apply_size(600, 600);
        host.fit_view();
        let narrow = host.stats().view_height;
        host.apply_size(1600, 600);
        let wide = host.stats().view_height;
        assert!(wide < narrow, "a wider drawing area fits the sheet at a smaller view height ({wide} < {narrow})");
        // Zoomed in by the user: a later resize leaves the view alone.
        let _ = host.session.zoom_about(0.5, aad_engine::geom::Vec2::new(8.0, 3.0));
        let zoomed = host.stats().view_height;
        host.apply_size(900, 600);
        assert_eq!(host.stats().view_height, zoomed, "the user's view is kept");
    }

    #[test]
    fn sample_line_and_wheel_zoom() {
        let mut host = Host::create(true);
        let before = host.stats().entities;
        assert!(before > 10, "sample drawing");
        assert!(host.mesh_vertices().unwrap() > 0, "mesh path");
        host.execute("line", &json!({ "points": [[0.0, 0.0], [2.0, 0.0]] })).unwrap();
        assert_eq!(host.stats().entities, before + 1);
        let height = host.stats().view_height;
        host.pointer(Pointer { x: 600.0, y: 400.0, action: POINTER_WHEEL, wheel: 1.0, mods: 0 }).unwrap();
        assert!(host.stats().view_height < height, "wheel zooms in");
        host.pointer(Pointer { x: 600.0, y: 400.0, action: POINTER_WHEEL, wheel: -1.0, mods: 0 }).unwrap();
        assert!((host.stats().view_height - height).abs() / height < 1e-6);
    }

    #[test]
    fn menu_and_command_line() {
        let mut host = Host::create(false);
        let menu: serde_json::Value = serde_json::from_str(&host.menu_json()).unwrap();
        let draw = menu.as_array().unwrap().iter().find(|m| m["title"] == "Draw").unwrap();
        let line = draw["items"].as_array().unwrap().iter().find(|i| i["id"] == "line").unwrap();
        assert_eq!(line["enabled"], true);
        assert!(!host.menu_json().contains("Cmd+"));
        assert!(host.menu_json().contains("Ctrl+N"));
        let before = host.stats().entities;
        host.submit("LINE 0,0 4,0").unwrap();
        assert_eq!(host.stats().entities, before + 1);
        let chrome: serde_json::Value = serde_json::from_str(&host.chrome_json()).unwrap();
        assert!(chrome["prompt"].as_str().unwrap().contains("Specify next point"));
        assert_eq!(chrome["command"], "line");
        assert_eq!(chrome["literalSpace"], false);
        assert_eq!(chrome["space"], "Model");
        assert_eq!(chrome["osnap"], true);
        assert_eq!(chrome["ortho"], false);
        assert_eq!(chrome["documents"][0]["active"], true);
        assert_eq!(chrome["documents"][0]["dirty"], true);
        assert!((chrome["polarang"].as_f64().unwrap() - 90.0).abs() < 1e-6);
        assert_eq!(chrome["osmode"], 4133);
        host.cancel_command().unwrap();
        let chrome: serde_json::Value = serde_json::from_str(&host.chrome_json()).unwrap();
        assert_eq!(chrome["prompt"], "Command:");
        assert_eq!(chrome["command"], "");
        assert!(host.submit("not-a-command").is_err());
        assert_eq!(host.stats().entities, before + 1);
        assert!(host.chrome_json().contains("unknown command"));
    }

    #[test]
    fn panels_list_block_reference_handles() {
        let mut host = Host::create(false);
        let line = host.execute("line", &json!({ "points": [[0.0, 0.0], [1.0, 0.0]] })).unwrap();
        let handle = line.get("handles").and_then(|v| v.as_array()).and_then(|a| a.first()).cloned()
            .or_else(|| line.get("handle").cloned())
            .unwrap_or(json!("1"));
        host.execute("block", &json!({ "name": "DOOR", "base": [0.0, 0.0], "handles": [handle], "keep": "delete" })).unwrap();
        host.execute("insert", &json!({ "name": "DOOR", "at": [2.0, 0.0] })).unwrap();
        let panels: serde_json::Value = serde_json::from_str(&host.panels_json()).unwrap();
        let blocks = panels["blocks"].as_array().unwrap();
        let door = blocks.iter().find(|b| b["name"] == "DOOR").expect("DOOR block");
        assert!(door["references"].as_u64().unwrap_or(0) >= 1);
        assert!(!door["handles"].as_array().unwrap().is_empty());
    }

    #[test]
    fn panels_follow_the_drawing_revision() {
        let mut host = Host::create(false);
        let first: serde_json::Value = serde_json::from_str(&host.panels_json()).unwrap();
        assert_eq!(first["properties"][0]["value"], "No selection");
        assert!(first["quick"].is_null());
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        host.execute("selectall", &json!({})).unwrap();
        let selected: serde_json::Value = serde_json::from_str(&host.panels_json()).unwrap();
        assert_eq!(selected["quick"]["title"], "Line");
        assert_eq!(selected["quick"]["lineweight"], "ByLayer");
        assert_eq!(selected["quick"]["transparency"], "ByLayer");
        assert!(selected["quick"]["fields"].as_array().unwrap().iter().any(|field| field["key"] == "length"));
        assert_eq!(first["layers"][0]["name"], "0");
        assert_eq!(first["layers"][0]["current"], true);
        let revision = first["revision"].as_u64().unwrap();
        host.execute("layer.set", &json!({ "name": "0", "color": 1 })).unwrap();
        let next: serde_json::Value = serde_json::from_str(&host.panels_json()).unwrap();
        assert!(next["revision"].as_u64().unwrap() > revision);
        assert_eq!(next["layers"][0]["colorName"], "Red");
        assert!(host.execute("layer.set", &json!({ "name": "0", "frozen": true })).is_err());
        let kept: serde_json::Value = serde_json::from_str(&host.panels_json()).unwrap();
        assert_eq!(kept["layers"][0]["frozen"], false);
        host.submit("insert").unwrap();
        host.submit("NO-SUCH-BLOCK").unwrap();
        assert!(host.chrome_json().contains("not found"));
        assert_eq!(host.stats().entities, 1);
    }

    #[test]
    fn an_open_started_on_a_worker_finishes_by_polling_and_a_bad_file_fails_cleanly() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        let dxf = std::env::temp_dir().join(format!("aad-host-async-{}.dxf", std::process::id()));
        host.execute("saveas", &json!({ "path": dxf.to_string_lossy() })).unwrap();
        let wait = |h: &mut Host| {
            for _ in 0..2000 {
                match h.open_poll() {
                    Ok(1) => std::thread::sleep(std::time::Duration::from_millis(5)),
                    other => return other,
                }
            }
            Err("timed out".into())
        };
        let mut opened = Host::create(false);
        assert_eq!(opened.open_poll(), Ok(0));
        opened.open_start(&dxf.to_string_lossy()).unwrap();
        assert!(opened.open_start(&dxf.to_string_lossy()).is_err(), "one open at a time");
        assert_eq!(wait(&mut opened), Ok(2));
        assert!(opened.stats().entities >= 1);
        assert_eq!(opened.open_poll(), Ok(0));
        let before = opened.stats().entities;
        opened.open_start("Z:/no/such/file.dxf").unwrap();
        assert!(wait(&mut opened).is_err());
        assert_eq!(opened.stats().entities, before, "a failed open leaves the drawing alone");
        assert!(opened.open_start("").is_err());
        let _ = std::fs::remove_file(dxf);
    }

    #[test]
    fn a_save_started_on_a_worker_writes_the_file_and_a_bad_path_fails_cleanly() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        let dxf = std::env::temp_dir().join(format!("aad-host-save-async-{}.dxf", std::process::id()));
        let wait = |h: &mut Host| {
            for _ in 0..2000 {
                match h.save_poll() {
                    Ok(1) => std::thread::sleep(std::time::Duration::from_millis(5)),
                    other => return other,
                }
            }
            Err("timed out".into())
        };
        assert_eq!(host.save_poll(), Ok(0));
        assert!(host.save_start(None).is_err(), "a new drawing has no file name");
        host.save_start(Some(&dxf.to_string_lossy())).unwrap();
        assert!(host.save_start(Some(&dxf.to_string_lossy())).is_err(), "one save at a time");
        assert_eq!(wait(&mut host), Ok(2));
        let mut reopened = Host::create(false);
        reopened.execute("open", &json!({ "path": dxf.to_string_lossy() })).unwrap();
        assert!(reopened.stats().entities >= 1);
        // The drawing now has a file name, so a quick save needs no path.
        host.save_start(None).unwrap();
        assert_eq!(wait(&mut host), Ok(2));
        host.save_start(Some("Z:/no/such/dir/x.dxf")).unwrap();
        assert!(wait(&mut host).is_err());
        assert_eq!(host.save_poll(), Ok(0));
        let _ = std::fs::remove_file(dxf);
    }

    #[test]
    fn saving_records_a_drawing_revision_and_the_command_notices_a_later_change() {
        let mut host = Host::create(false);
        let dir = std::env::temp_dir().join(format!("aad-host-rev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.dxf").to_string_lossy().to_string();
        assert!(host.execute("drawing.revision", &json!({})).is_err(), "no file yet");
        host.execute("line", &json!({ "points": [[0.0, 0.0], [5.0, 0.0]] })).unwrap();
        host.execute("saveas", &json!({ "path": path })).unwrap();
        let r = host.execute("drawing.revision", &json!({})).unwrap();
        assert_eq!(r["status"], json!("current"));
        assert_eq!(r["recorded"]["entities"], json!(1));
        assert_eq!(r["recorded"]["file"], json!("plan.dxf"));
        // The worker-thread save records one as well.
        host.execute("line", &json!({ "points": [[0.0, 1.0], [5.0, 1.0]] })).unwrap();
        host.save_start(None).unwrap();
        for _ in 0..2000 {
            if host.save_poll() != Ok(1) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let r = host.execute("drawing.revision", &json!({})).unwrap();
        assert_eq!(r["status"], json!("current"));
        assert_eq!(r["recorded"]["entities"], json!(2));
        std::fs::write(&path, b"edited elsewhere").unwrap();
        assert_eq!(host.execute("drawing.revision", &json!({})).unwrap()["status"], json!("changed"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn open_reads_a_saved_dwg() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        let dwg = std::env::temp_dir().join(format!("aad-host-open-{}.dwg", std::process::id()));
        let dxf = std::env::temp_dir().join(format!("aad-host-open-{}.dxf", std::process::id()));
        host.execute("saveas", &json!({ "path": dwg.to_string_lossy() })).unwrap();
        host.execute("saveas", &json!({ "path": dxf.to_string_lossy() })).unwrap();
        let mut opened = Host::create(false);
        opened.execute("open", &json!({ "path": dwg.to_string_lossy() })).unwrap();
        assert!(opened.stats().entities >= 1);
        opened.execute("open", &json!({ "path": dxf.to_string_lossy() })).unwrap();
        assert!(opened.stats().entities >= 1);
        let _ = std::fs::remove_file(&dwg);
        let _ = std::fs::remove_file(&dxf);
    }

    #[test]
    fn clicks_draw_and_pan_moves_the_view() {
        let mut host = Host::create(false);
        host.submit("line").unwrap();
        host.pointer(Pointer { x: 100.0, y: 200.0, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: 100.0, y: 200.0, action: POINTER_LEFT_UP, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: 400.0, y: 200.0, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        assert_eq!(host.stats().entities, 1);
        host.pointer(Pointer { x: 0.0, y: 0.0, action: POINTER_RIGHT, wheel: 0.0, mods: 0 }).unwrap();
        assert!(host.take_popup().is_empty(), "a right-click while a command runs is Enter");
        assert!(host.chrome_json().contains("Command:"));
        host.pointer(Pointer { x: 12.0, y: 12.0, action: POINTER_RIGHT, wheel: 0.0, mods: 0 }).unwrap();
        let menu: serde_json::Value = serde_json::from_str(&host.take_popup()).unwrap();
        assert_eq!(menu["kind"], "command");
        host.submit("line").unwrap();
        host.pointer(Pointer { x: 12.0, y: 12.0, action: POINTER_RIGHT, wheel: 0.0, mods: MOD_SHIFT }).unwrap();
        let snaps: serde_json::Value = serde_json::from_str(&host.take_popup()).unwrap();
        assert_eq!(snaps["kind"], "snap");
        assert!(host.chrome_json().contains("Specify"));
        let before = host.screen_to_world(400.0, 200.0).unwrap();
        host.pointer(Pointer { x: 10.0, y: 10.0, action: POINTER_MIDDLE_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: 80.0, y: 10.0, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: 80.0, y: 10.0, action: POINTER_MIDDLE_UP, wheel: 0.0, mods: 0 }).unwrap();
        let after = host.screen_to_world(400.0, 200.0).unwrap();
        assert!(before.dist(after) > 1e-6, "middle drag pans");
        assert_eq!(host.stats().entities, 1);
        host.set_dark(false).unwrap();
        host.set_dark(true).unwrap();
    }

    #[test]
    fn double_click_edits_text_and_middle_double_zooms_extents() {
        let mut host = Host::create(false);
        host.execute("text", &json!({ "at": [0.0, 0.0], "text": "Hi", "height": 1.0 })).unwrap();
        host.execute("selectall", &json!({})).unwrap();
        host.execute("zoom.extents", &json!({})).unwrap();
        let at = host.world_to_screen(Vec2::ZERO).unwrap();
        host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_LEFT_DOUBLE, wheel: 0.0, mods: 0 }).unwrap();
        assert!(host.chrome_json().contains("new text"), "a double-click on text starts TEXTEDIT");
        host.cancel_command().unwrap();
        let fitted = host.stats().view_height;
        host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_WHEEL, wheel: 2.0, mods: 0 }).unwrap();
        assert!(host.stats().view_height < fitted);
        host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_MIDDLE_DOUBLE, wheel: 0.0, mods: 0 }).unwrap();
        assert!((host.stats().view_height - fitted).abs() / fitted < 1e-6, "a middle double-click zooms to the extents");
    }

    fn mesh_has_rgb(mesh: &MeshData, rgb: [u8; 3]) -> bool {
        let hit = |bytes: &[u8], stride: usize, at: usize| {
            bytes.chunks(stride).any(|v| v.len() >= at + 3 && v[at] == rgb[0] && v[at + 1] == rgb[1] && v[at + 2] == rgb[2])
        };
        hit(&mesh.lines, mesh::VERTEX_SIZE, 8) || hit(&mesh.tris, mesh::VERTEX_SIZE, 8) || hit(&mesh.quads, mesh::INSTANCE_SIZE, 16)
    }

    #[test]
    fn light_paper_draws_colour_7_black() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        let (dark_key, dark_mesh, _, _) = host.mesh().unwrap();
        assert!(mesh_has_rgb(&dark_mesh, [255, 255, 255]), "colour 7 is white on the black drawing area");
        host.dark = false;
        let (light_key, light_mesh, _, _) = host.mesh().unwrap();
        assert_ne!(light_key, dark_key, "the canvas reuploads when the sheet colour changes");
        assert!(mesh_has_rgb(&light_mesh, [0, 0, 0]), "colour 7 is black on the white drawing area");
        assert!(!mesh_has_rgb(&light_mesh, [255, 255, 255]), "the white stroke is replaced");
    }

    #[test]
    fn a_ray_is_drawn_across_the_view() {
        let mut host = Host::create(false);
        let empty = host.mesh_vertices().unwrap();
        host.execute("ray", &json!({ "base": [0.0, 0.0], "through": [10.0, 0.0] })).unwrap();
        assert!(host.mesh_vertices().unwrap() > empty, "a ray is clipped into the canvas mesh");
    }

    #[test]
    fn rejected_input_leaves_the_drawing() {
        let mut host = Host::create(false);
        let before = host.stats().entities;
        let height = host.stats().view_height;
        assert!(host.execute("", &json!({})).is_err());
        assert!(host.execute("no-such-command", &json!({})).is_err());
        assert!(host.execute("line", &json!({ "points": "nope" })).is_err());
        assert!(host.pointer(Pointer { x: f64::NAN, y: 0.0, action: POINTER_WHEEL, wheel: 1.0, mods: 0 }).is_err());
        assert_eq!(host.stats().entities, before);
        assert_eq!(host.stats().view_height, height);
    }

    #[test]
    fn ffi_executes_a_line() {
        let host = ffi::create(1);
        assert!(!host.is_null());
        let rc = unsafe { ffi::execute(host, c"line".as_ptr(), c"{\"points\":[[0,0],[1,0]]}".as_ptr()) };
        assert_eq!(rc, 0, "{}", ffi::error_text(host));
        let mut stats = Stats { entities: 0, _pad: 0, view_height: 0.0, width: 0, height: 0 };
        assert_eq!(unsafe { ffi::stats(host, &mut stats) }, 0);
        assert!(stats.entities > 1);
        unsafe { ffi::destroy(host) };
    }

    #[test]
    fn endpoint_snap_places_the_line() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        host.execute("zoom.extents", &json!({})).unwrap();
        host.submit("line").unwrap();
        let near = host.world_to_screen(Vec2::new(0.02, 0.02)).unwrap();
        host.pointer(Pointer { x: near.x, y: near.y, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        assert!(host.session.cursor.dist(Vec2::ZERO) < 1e-6, "the cursor snaps to the endpoint");
        host.pointer(Pointer { x: near.x, y: near.y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        let far = host.world_to_screen(Vec2::new(4.0, 2.0)).unwrap();
        host.pointer(Pointer { x: far.x, y: far.y, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        let rubber = overlay::mesh(
            &host.session,
            Some(&overlay::Cursor { aim: host.aim, raw: host.raw, snap: host.snap.map(|hit| hit.point), hot: host.hot }),
            host.paper(),
            host.height,
            Vec2::ZERO,
            None,
            host.width,
            host.show_ucs,
            f64::from(host.pixels_per_point),
        );
        assert!(rubber.quad_instances() > 4, "the rubber band and crosshair are drawn");
        host.pointer(Pointer { x: far.x, y: far.y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: 0.0, y: 0.0, action: POINTER_RIGHT, wheel: 0.0, mods: 0 }).unwrap();
        let doc = host.session.doc().unwrap();
        let space = doc.space(&aad_engine::doc::Space::Model).unwrap();
        let snapped = space.iter().any(|entity| match &entity.kind {
            aad_engine::doc::EntityKind::Line(line) => line.a.xy().dist(Vec2::ZERO) < 1e-6 && line.b.xy().dist(Vec2::new(4.0, 2.0)) < 1.0,
            _ => false,
        });
        assert!(snapped, "the new line starts on the snapped endpoint");
    }

    #[test]
    fn dynamic_input_locks_the_distance_and_places_the_point() {
        let mut host = Host::create(false);
        host.submit("line").unwrap();
        host.submit("0,0").unwrap();
        assert_eq!(host.dyn_tab("40").as_deref(), Some("40<"));
        host.set_draft("5<").unwrap();
        let up = host.world_to_screen(Vec2::new(0.0, 20.0)).unwrap();
        host.pointer(Pointer { x: up.x, y: up.y, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        assert!((host.aim - Vec2::new(0.0, 5.0)).len() < 1e-6, "a locked distance holds the cursor");
        let tip = host.dyn_tip().unwrap();
        assert!(tip.contains("5"), "{tip}");
        let drawn = overlay::mesh(
            &host.session,
            Some(&overlay::Cursor { aim: host.aim, raw: host.raw, snap: None, hot: None }),
            host.paper(),
            host.height,
            Vec2::ZERO,
            Some(&tip),
            host.width,
            host.show_ucs,
            f64::from(host.pixels_per_point),
        );
        assert!(drawn.quad_instances() > 0, "the tooltip is stroked");
        host.submit("5<90").unwrap();
        let doc = host.session.doc().unwrap();
        let space = doc.space(&aad_engine::doc::Space::Model).unwrap();
        let placed = space.iter().any(|entity| match &entity.kind {
            aad_engine::doc::EntityKind::Line(line) => line.a.xy().dist(Vec2::ZERO) < 1e-6 && line.b.xy().dist(Vec2::new(0.0, 5.0)) < 1e-6,
            _ => false,
        });
        assert!(placed, "Enter places the locked polar point");
    }

    #[test]
    fn grip_cycle_moves_the_selected_line() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        host.execute("zoom.extents", &json!({})).unwrap();
        let at = host.world_to_screen(Vec2::ZERO).unwrap();
        let click = |host: &mut Host, at: Vec2| {
            host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
            host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_LEFT_UP, wheel: 0.0, mods: 0 }).unwrap();
        };
        click(&mut host, at);
        click(&mut host, at);
        assert!(host.hot.is_some(), "the second click picks the grip");
        assert!(host.grip_cycle());
        assert_eq!(host.hot.unwrap().mode, aad_engine::grips::GripMode::Move);
        let to = host.world_to_screen(Vec2::new(0.0, 5.0)).unwrap();
        click(&mut host, to);
        let doc = host.session.doc().unwrap();
        let space = doc.space(&aad_engine::doc::Space::Model).unwrap();
        let moved = space.iter().any(|entity| match &entity.kind {
            aad_engine::doc::EntityKind::Line(line) => line.a.xy().dist(Vec2::new(0.0, 5.0)) < 1e-6 && line.b.xy().dist(Vec2::new(10.0, 5.0)) < 1e-6,
            _ => false,
        });
        assert!(moved, "move mode shifts the whole line");
    }

    #[test]
    fn keyboard_nudge_and_activate_select_without_a_pointer() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        host.execute("zoom.extents", &json!({})).unwrap();
        let at = host.world_to_screen(Vec2::ZERO).unwrap();
        host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        let before = host.screen.expect("cursor on screen");
        host.key_nudge(1, 0, false, false).unwrap();
        let after = host.screen.expect("cursor on screen");
        assert!(after.x > before.x, "arrow keys move the screen cursor");
        let on_line = host.world_to_screen(Vec2::new(5.0, 0.0)).unwrap();
        host.pointer(Pointer { x: on_line.x, y: on_line.y, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        host.key_activate(false).unwrap();
        let selected = host.session.state().unwrap().selection.len();
        assert!(selected >= 1, "Insert picks the entity under the keyboard cursor");
        let center = host.session.state().unwrap().view().center;
        host.key_nudge(2, 0, true, false).unwrap();
        let moved = host.session.state().unwrap().view().center;
        assert!(center.dist(moved) > 1e-9, "Ctrl+arrow pans");
    }

    #[test]
    fn a_typed_point_applies_the_hot_grip_and_escape_clears_it() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        host.execute("zoom.extents", &json!({})).unwrap();
        let at = host.world_to_screen(Vec2::ZERO).unwrap();
        host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_LEFT_UP, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: at.x, y: at.y, action: POINTER_LEFT_UP, wheel: 0.0, mods: 0 }).unwrap();
        assert!(host.hot.is_some());
        assert!(!host.grip_point("   "));
        assert!(host.grip_point("nope"));
        assert!(host.hot.is_some(), "a non-point leaves the grip hot");
        assert!(host.chrome_json().contains("Requires a point"));
        assert!(host.grip_point("@0,5"));
        assert!(host.hot.is_none());
        let doc = host.session.doc().unwrap();
        let space = doc.space(&aad_engine::doc::Space::Model).unwrap();
        let stretched = space.iter().any(|entity| match &entity.kind {
            aad_engine::doc::EntityKind::Line(line) => line.a.xy().dist(Vec2::new(0.0, 5.0)) < 1e-6 && line.b.xy().dist(Vec2::new(10.0, 0.0)) < 1e-6,
            _ => false,
        });
        assert!(stretched, "stretch moves the picked endpoint");
        let moved = host.world_to_screen(Vec2::new(0.0, 5.0)).unwrap();
        host.pointer(Pointer { x: moved.x, y: moved.y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        host.pointer(Pointer { x: moved.x, y: moved.y, action: POINTER_LEFT_UP, wheel: 0.0, mods: 0 }).unwrap();
        assert!(host.hot.is_some(), "the endpoint is hot again");
        host.cancel_command().unwrap();
        assert!(host.hot.is_none());
        assert!(host.chrome_json().contains("*Cancel*"));
        assert!(!host.grip_point("1,0"));
    }

    #[test]
    fn tab_cycles_object_snaps_and_a_form_is_json() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        host.execute("line", &json!({ "points": [[0.2, 0.2], [0.2, 8.0]] })).unwrap();
        host.execute("zoom.extents", &json!({})).unwrap();
        host.submit("line").unwrap();
        let near = host.world_to_screen(Vec2::new(0.1, 0.1)).unwrap();
        host.pointer(Pointer { x: near.x, y: near.y, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        host.snap_tab().unwrap();
        assert!(host.aim.is_finite());
        assert!(host.snap_cycle.is_some());
        let form = host.form_json("electrical.schedule").unwrap();
        assert!(form.contains("Electrical Schedule"));
        assert!(host.form_json("line").is_none());
    }

    #[test]
    fn layer_query_is_the_authoritative_table() {
        let host = ffi::create(0);
        let run = |command: &str, params: &str| {
            let command = std::ffi::CString::new(command).unwrap();
            let params = std::ffi::CString::new(params).unwrap();
            unsafe { ffi::execute(host, command.as_ptr(), params.as_ptr()) }
        };
        assert_eq!(run("layer.new", r#"{"name":"Walls","plot":false,"transparency":40,"description":"exterior"}"#), 0);
        assert_eq!(run("layer", "{}"), 0);
        let table: serde_json::Value = serde_json::from_str(&ffi::result_text(host)).unwrap();
        let walls = table["layers"].as_array().unwrap().iter().find(|layer| layer["name"] == "Walls").unwrap();
        assert_eq!(walls["plot"], false);
        assert_eq!(walls["transparency"], 40);
        assert_eq!(walls["description"], "exterior");
        assert_eq!(walls["used"], false);
        assert!(table["linetypes"].as_array().unwrap().iter().any(|name| name == "Continuous"));
        assert_eq!(run("layer.set", r#"{"name":"0","frozen":true}"#), 1);
        assert!(ffi::result_text(host).is_empty(), "a rejected command clears the previous result");
        assert!(ffi::error_text(host).contains("cannot freeze"));
        assert_eq!(run("layer.delete", r#"{"name":"0"}"#), 1);
        assert!(ffi::error_text(host).contains("cannot delete"));
        unsafe { ffi::destroy(host) };
    }

    #[test]
    fn a_plot_started_on_a_worker_writes_the_pdf_and_bad_arguments_fail_cleanly() {
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        let pdf = std::env::temp_dir().join(format!("aad-host-plot-async-{}.pdf", std::process::id()));
        let wait = |h: &mut Host| {
            for _ in 0..2000 {
                match h.plot_poll() {
                    Ok(1) => std::thread::sleep(std::time::Duration::from_millis(5)),
                    other => return other,
                }
            }
            Err("timed out".into())
        };
        assert_eq!(host.plot_poll(), Ok(0));
        assert!(host.plot_start("line", &json!({})).is_err(), "only plotting commands");
        assert!(host.plot_start("plot", &json!({ "window": [0.0, 0.0, 1.0] })).is_err(), "a malformed window is refused up front");
        host.plot_start("plot", &json!({ "path": pdf.to_string_lossy() })).unwrap();
        assert!(host.plot_start("plot", &json!({})).is_err(), "one plot at a time");
        assert_eq!(wait(&mut host), Ok(2));
        let bytes = std::fs::read(&pdf).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        host.plot_start("plot", &json!({ "path": "Z:/no/such/dir/x.pdf" })).unwrap();
        assert!(wait(&mut host).is_err());
        assert_eq!(host.plot_poll(), Ok(0));
        let _ = std::fs::remove_file(pdf);
    }

    #[test]
    fn plot_preview_is_a_pdf() {
        let mut host = Host::create(true);
        let pdf = host.pdf_bytes().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.windows(4).any(|w| w == b"%%EOF") || pdf.ends_with(b"%%EOF\n") || pdf.ends_with(b"%%EOF"));
    }

    #[test]
    fn a_slash_request_reports_when_the_assistant_runtime_is_absent() {
        let mut host = Host::create(false);
        if host.assistant.available() {
            assert!(host.submit("/ layers").is_ok());
            assert_eq!(host.assistant.phase(), "thinking");
        } else {
            assert!(host.submit("/ layers").is_err());
            assert!(host.chrome_json().contains("not included"));
            assert_eq!(host.assistant.phase(), "idle");
        }
    }

    #[test]
    fn the_ucs_icon_click_opens_its_menu() {
        let mut host = Host::create(false);
        let y = f64::from(host.stats().height) - 40.0;
        host.pointer(Pointer { x: 40.0, y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        let popup = host.take_popup();
        assert!(popup.contains("Hide UCS Icon"), "{popup}");
        host.set_ucs(false).unwrap();
        host.pointer(Pointer { x: 40.0, y, action: POINTER_LEFT_DOWN, wheel: 0.0, mods: 0 }).unwrap();
        assert!(host.take_popup().is_empty());
    }

    #[test]
    fn plot_window_request_json_is_exposed_to_the_shell() {
        let mut host = Host::create(false);
        assert!(host.plot_request_json().is_empty());
        host.session.plot_request = Some(aad_engine::PlotRequest { window: None });
        assert!(host.plot_request_json().contains("null"), "{}", host.plot_request_json());
        host.session.plot_request = Some(aad_engine::PlotRequest { window: Some([1.0, 2.0, 3.0, 4.0]) });
        let json = host.plot_request_json();
        assert!(json.contains('1') && json.contains('4'), "{json}");
        host.clear_plot_request();
        assert!(host.plot_request_json().is_empty());
        assert!(host.session.plot_request.is_none());
    }

    #[test]
    fn ui_inspect_menu_list_pointer_and_screenshot_work_on_the_control_channel() {
        let mut host = Host::create(false);
        host.apply_size(640, 480);
        let inspect = host.control_one("ui.inspect", &json!({}));
        assert_eq!(inspect["ok"], true, "{inspect}");
        assert_eq!(inspect["result"]["shell"], "winui");
        assert_eq!(inspect["result"]["canvas"]["width"], 640);
        let menus = host.control_one("ui.menu.list", &json!({}));
        assert_eq!(menus["ok"], true, "{menus}");
        assert!(menus["result"].as_array().unwrap().iter().any(|m| m["label"] == "File"));
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        let click = host.control_one("ui.pointer", &json!({ "x": 5.0, "y": 0.0, "space": "world" }));
        assert_eq!(click["ok"], true, "{click}");
        assert_eq!(host.session.selection().len(), 1);
        let path = std::env::temp_dir().join(format!("aad-host-ui-shot-{}.png", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let shot = host.control_one("ui.screenshot", &json!({ "path": path.to_string_lossy() }));
        assert_eq!(shot["ok"], true, "{shot}");
        assert_eq!(shot["result"]["drawing"], true);
        assert!(path.is_file(), "missing {}", path.display());
        let _ = std::fs::remove_file(&path);
        let chrome = host.control_one("ui.click", &json!({ "x": 1.0, "y": 1.0 }));
        assert_eq!(chrome["ok"], false);
        assert!(chrome["error"].as_str().unwrap_or("").contains("egui"));
    }

    #[test]
    fn ui_render_writes_a_png_on_the_control_channel() {
        let path = std::env::temp_dir().join(format!("aad-host-ui-render-{}.png", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        let reply = host.control_one(
            "ui.render",
            &json!({ "path": path.to_string_lossy(), "width": 64, "height": 48 }),
        );
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["result"]["width"], 64);
        assert_eq!(reply["result"]["height"], 48);
        assert!(reply["result"]["bytes"].as_u64().unwrap_or(0) > 0);
        assert!(path.is_file(), "missing {}", path.display());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn app_open_save_and_quit_work_on_the_control_channel() {
        let path = std::env::temp_dir().join(format!("aad-host-app-open-{}.dxf", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut host = Host::create(false);
        host.execute("line", &json!({ "points": [[0.0, 0.0], [3.0, 0.0]] })).unwrap();
        host.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap();
        let mut opened = Host::create(false);
        let open = opened.control_one("app.open", &json!({ "path": path.to_string_lossy() }));
        assert_eq!(open["ok"], true, "{open}");
        assert!(opened.stats().entities >= 1);
        let save = opened.control_one("app.save", &json!({}));
        assert_eq!(save["ok"], true, "{save}");
        let missing = opened.control_one("app.open", &json!({}));
        assert_eq!(missing["ok"], false);
        assert!(missing["error"].as_str().unwrap_or("").contains("missing path"));
        let quit = opened.control_one("app.quit", &json!({}));
        assert_eq!(quit["ok"], true, "{quit}");
        let chrome: serde_json::Value = serde_json::from_str(&opened.chrome_json()).unwrap();
        assert_eq!(chrome["quit"], true);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_control_channel_runs_a_command_on_the_session_thread() {
        let mut host = Host::create(false);
        let port = host.listen(0).unwrap();
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        stream.set_write_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        use std::io::Write;
        writeln!(stream, r#"{{"id":7,"method":"engine.execute","params":{{"command":"line","params":{{"points":[[0,0],[2,0]]}}}}}}"#).unwrap();
        stream.set_nonblocking(true).unwrap();
        let start = std::time::Instant::now();
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        while start.elapsed() < std::time::Duration::from_secs(3) && !got.contains(&b'\n') {
            host.pump().unwrap();
            match std::io::Read::read(&mut stream, &mut buf) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(e) => panic!("{e}"),
            }
        }
        let text = String::from_utf8(got).unwrap();
        assert!(text.contains("\"ok\":true") && text.contains("\"id\":7"), "{text}");
        assert!(host.stats().entities >= 1);
    }

    #[test]
    fn the_control_channel_waits_for_a_line_written_after_accept() {
        let mut host = Host::create(false);
        let port = host.listen(0).unwrap();
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(80));
        stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        stream.set_write_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        use std::io::Write;
        writeln!(stream, r#"{{"id":4,"method":"cmdline.input","params":{{"text":"line"}}}}"#).unwrap();
        stream.set_nonblocking(true).unwrap();
        let start = std::time::Instant::now();
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        while start.elapsed() < std::time::Duration::from_secs(3) && !got.contains(&b'\n') {
            host.pump().unwrap();
            match std::io::Read::read(&mut stream, &mut buf) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(e) => panic!("{e}"),
            }
        }
        let text = String::from_utf8(got).unwrap();
        assert!(text.contains("\"ok\":true") && text.contains("\"id\":4"), "{text}");
        assert_eq!(host.session.running.as_ref().map(|running| running.id.as_str()), Some("line"));
    }

    #[test]
    fn dyn_chrome_reports_the_point_prompt_fields() {
        let mut host = Host::create(false);
        host.submit("LINE 0,0 4,0").unwrap();
        host.pointer(Pointer { x: 100.0, y: 40.0, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        let chrome: serde_json::Value = serde_json::from_str(&host.chrome_json()).unwrap();
        assert_eq!(chrome["dyn"]["on"], true);
        assert_eq!(chrome["dyn"]["x"], 100.0);
        assert_eq!(chrome["dyn"]["aLabel"], "Length");
        assert_eq!(chrome["dyn"]["bLabel"], "Angle");
        assert!(!chrome["dyn"]["a"].as_str().unwrap_or("").is_empty());
    }

    #[test]
    fn dyn_chrome_shows_the_typed_command_not_the_coordinates_and_leaves_assistant_requests_to_the_bar() {
        let mut host = Host::create(false);
        let mv = |h: &mut Host| h.pointer(Pointer { x: 100.0, y: 40.0, action: POINTER_MOVE, wheel: 0.0, mods: 0 }).unwrap();
        let dynamic = |h: &Host| -> serde_json::Value { serde_json::from_str::<serde_json::Value>(&h.chrome_json()).unwrap()["dyn"].clone() };
        // Idle with nothing typed: no X and Y readout follows the cursor.
        mv(&mut host);
        assert_eq!(dynamic(&host)["on"], false);
        // A command being typed shows beside the cursor.
        host.set_draft("lin").unwrap();
        let d = dynamic(&host);
        assert_eq!((d["on"].clone(), d["aLabel"].clone(), d["a"].clone(), d["bLabel"].clone()), (json!(true), json!("Command"), json!("lin"), json!("")));
        assert_eq!(d["x"], 100.0);
        // An assistant request, `/` or a sentence, stays in the command bar.
        host.set_draft("/ what layers are there").unwrap();
        assert_eq!(dynamic(&host)["on"], false);
        host.set_draft("what is in this drawing").unwrap();
        assert_eq!(dynamic(&host)["on"], false);
        host.set_draft("").unwrap();
        // While a command waits for a point with no base point yet there is still no coordinate readout.
        host.submit("LINE").unwrap();
        mv(&mut host);
        assert_eq!(dynamic(&host)["on"], false);
        // A typed answer shows as input, and the same words are not taken for a request while a command runs.
        host.set_draft("what is this").unwrap();
        let d = dynamic(&host);
        assert_eq!((d["on"].clone(), d["a"].clone()), (json!(true), json!("what is this")));
    }

    #[test]
    fn clear_log_keeps_the_drawing_and_the_running_command() {
        let mut host = Host::create(false);
        host.submit("LINE 0,0 4,0").unwrap();
        let entities = host.stats().entities;
        assert!(entities >= 1);
        assert!(host.session.running.is_some());
        assert!(!host.session.log.is_empty());
        host.clear_log().unwrap();
        assert!(host.session.log.is_empty());
        assert_eq!(host.stats().entities, entities);
        assert!(host.session.running.is_some());
        let chrome: serde_json::Value = serde_json::from_str(&host.chrome_json()).unwrap();
        assert!(chrome["history"].as_array().is_some_and(|rows| rows.is_empty()));
        assert_eq!(chrome["command"], "line");
    }
}
