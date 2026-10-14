//! Windows shell host.
//!
//! The WinUI app calls the C ABI in [`ffi`]. This crate owns the [`aad_engine::Session`], packs
//! the display list with [`aad_render::mesh`], and on Windows paints that mesh into a child
//! window. It exposes the registry menu, the command line, form descriptions and grip editing.
//!
//! `unsafe` is denied except in the ABI, the child window, and the swap-chain surface.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod chrome;
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

/// One editing session and, on Windows, its drawing surface.
pub struct Host {
    session: Session,
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
}

impl Host {
    /// `sample` opens the built-in sample drawing. Otherwise the session starts empty.
    pub fn create(sample: bool) -> Self {
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
        }
    }

    pub fn execute(&mut self, command: &str, params: &Value) -> Result<Value, String> {
        if command.is_empty() || command.len() > 128 {
            return Err("missing command".into());
        }
        self.result.clear();
        let value = self.session.execute(command, params).map_err(|e| e.to_string())?;
        let text = value.to_string();
        if text.len() > 8 * 1_024 * 1_024 {
            return Err("result is too large".into());
        }
        self.result = text;
        Ok(value)
    }

    /// Run one command-line line, the same path as typing it and pressing Enter.
    pub fn submit(&mut self, text: &str) -> Result<(), String> {
        self.exclusive(|host| host.submit_inner(text))
    }

    /// Escape. A hot grip is cleared; otherwise the running command is cancelled.
    pub fn cancel_command(&mut self) -> Result<(), String> {
        self.exclusive(|host| {
            host.draft.clear();
            if host.hot.take().is_some() {
                host.session.echo("*Cancel*");
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
        chrome::chrome_json(&self.session, cursor)
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
            let inside = viewport.as_ref().is_some_and(|(_, view)| (paper.x - view.center.x).abs() <= view.width / 2.0 && (paper.y - view.center.y).abs() <= view.height / 2.0);
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
        aad_engine::dyninput::parse(text, &frame)
            .and_then(|entry| aad_engine::dyninput::complete(&entry, &frame, self.aim, self.session.last_point))
            .unwrap_or_else(|| text.to_string())
    }

    /// The line drawn beside the cursor while a command is running.
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
        let mut built = mesh::build_mesh(&list, origin, self.paper(), lw);
        let world_width = view.height * f64::from(self.width.max(1)) / f64::from(self.height.max(1));
        mesh::append_infinite(&mut built, &list, origin, view.center, world_width, view.height, self.paper());
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
        let key = state.revision ^ origin.x.to_bits() ^ origin.y.to_bits().rotate_left(1);
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
        let _ = self.session.zoom_extents();
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
        self.in_call = true;
        let result = (|| {
            window::raise(self.hwnd);
            for ev in self.bridge.drain() {
                self.pointer(ev)?;
            }
            if self.gpu.is_some() {
                self.redraw()?;
            }
            Ok(())
        })();
        self.in_call = false;
        result
    }

    pub fn redraw(&mut self) -> Result<(), String> {
        self.pixels_per_point = window::dpi_scale(self.hwnd);
        let (key, built, uniform, origin) = self.mesh()?;
        let paper = self.paper();
        let cursor =
            self.screen.is_some().then_some(overlay::Cursor { aim: self.aim, raw: self.raw, snap: self.snap.map(|hit| hit.point), hot: self.hot });
        let tip = self.dyn_tip();
        let overlay = overlay::mesh(&self.session, cursor.as_ref(), paper, self.height, origin, tip.as_deref());
        let gpu = self.gpu.as_mut().ok_or("no drawing surface")?;
        gpu.set_clear(paper);
        gpu.set_overlay(&overlay);
        gpu.present(key, &built, &uniform)
    }

    fn apply_size(&mut self, width: u32, height: u32) {
        self.width = width.clamp(1, 8192);
        self.height = height.clamp(1, 8192);
        self.session.viewport_px = (f64::from(self.width), f64::from(self.height));
    }
}

#[cfg(windows)]
impl Drop for Host {
    fn drop(&mut self) {
        self.gpu.take();
        if self.hwnd != 0 {
            window::destroy(self.hwnd);
            self.hwnd = 0;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, unsafe_code)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn panels_follow_the_drawing_revision() {
        let mut host = Host::create(false);
        let first: serde_json::Value = serde_json::from_str(&host.panels_json()).unwrap();
        assert_eq!(first["properties"][0]["value"], "No selection");
        assert!(first["quick"].is_null());
        host.execute("line", &json!({ "points": [[0.0, 0.0], [10.0, 0.0]] })).unwrap();
        host.execute("selectall", &json!({})).unwrap();
        let selected: serde_json::Value = serde_json::from_str(&host.panels_json()).unwrap();
        assert_eq!(selected["quick"]["title"], "Line");
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
    fn plot_preview_is_a_pdf() {
        let mut host = Host::create(true);
        let pdf = host.pdf_bytes().unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(pdf.windows(4).any(|w| w == b"%%EOF") || pdf.ends_with(b"%%EOF\n") || pdf.ends_with(b"%%EOF"));
    }
}
