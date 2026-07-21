//! The egui desktop shell — command line, docked panels, and the drawing
//! viewport (ADR-0001: egui chrome over the custom canvas).
//!
//! Feature-gated (`--features gui`). This is the one part of `aadt-app` that
//! **cannot be verified headlessly** — it opens a window. It is deliberately a
//! thin driver of [`Session`]: the command line calls [`Session::feed`], the
//! panels read [`Session::takeoff`], and the viewport paints
//! [`Session::drawlist`]. Everything it drives is already tested.
//!
//! The viewport paints the draw list with egui's own 2-D painter (line
//! segments) — correct and interactive for working drawings. The custom
//! `wgpu`-callback viewport for 60 fps at 100k+ entities (spec §4.3) drops in
//! behind the same [`Session::drawlist`] later; the drawing, camera, and command
//! wiring proven here do not change.

use aadt_geometry::Point2;
use aadt_model::Document;
use aadt_render::{Camera, DrawCommand, DrawList};
use eframe::egui;

use crate::Session;

/// Open the desktop shell on `doc` and run until the window closes.
pub fn run(doc: Document) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AADT")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "AADT",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(AadtApp::new(doc)))
        }),
    )
}

struct AadtApp {
    session: Session,
    command: String,
    camera: Camera,
    fitted: bool,
}

impl AadtApp {
    fn new(doc: Document) -> Self {
        let mut session = Session::with_builtins();
        session.set_document(doc);
        Self {
            session,
            command: String::new(),
            camera: Camera::new((800.0, 600.0)),
            fitted: false,
        }
    }

    /// Run the current command line through the session and clear it.
    fn submit(&mut self) {
        if !self.command.trim().is_empty() {
            self.session.feed(&self.command);
            self.command.clear();
        }
    }
}

impl eframe::App for AadtApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Command line (primary surface, spec §2) ---
        egui::TopBottomPanel::top("command").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("aadt>").monospace().strong());
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.command)
                        .desired_width(f32::INFINITY)
                        .hint_text("LINE 0 0 10 0 · CIRCLE 5 5 2 · LAYER A-WALL · UNDO · LIST"),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.submit();
                    resp.request_focus();
                }
            });
        });

        // --- Left dock: drawing + takeoff ---
        egui::SidePanel::left("info")
            .default_width(240.0)
            .show(ctx, |ui| {
                ui.heading("Drawing");
                ui.label(format!("Entities: {}", self.session.document().len()));
                let layers = layer_summary(self.session.document());
                if !layers.is_empty() {
                    ui.collapsing("Layers", |ui| {
                        for (name, n) in layers {
                            ui.label(format!("{name}  ({n})"));
                        }
                    });
                }

                ui.separator();
                ui.heading("Takeoff");
                let t = self.session.takeoff();
                ui.label(format!("Walls: {}", t.walls.len()));
                ui.label(format!("Rooms: {}", t.rooms.len()));
                ui.label(format!("Wall length: {:.3}", t.total_wall_length()));
                ui.label(format!("Net wall area: {:.3}", t.total_wall_net_area()));
                ui.label(format!("Room area: {:.3}", t.total_room_area()));

                ui.separator();
                if ui.button("Fit view").clicked() {
                    self.fitted = false;
                }
                ui.small("drag to pan · scroll to zoom");
            });

        // --- Bottom: last feedback ---
        egui::TopBottomPanel::bottom("log").show(ctx, |ui| {
            if let Some(fb) = self.session.transcript().last() {
                if let Some(msg) = fb.message() {
                    if fb.is_err() {
                        ui.colored_label(egui::Color32::from_rgb(220, 90, 70), msg);
                    } else {
                        ui.monospace(msg);
                    }
                }
            }
        });

        // --- Central viewport ---
        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, resp) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            self.camera.viewport = (rect.width() as f64, rect.height() as f64);

            if !self.fitted {
                self.camera.fit_document(self.session.document(), 0.08);
                self.fitted = true;
            }
            handle_pan_zoom(&mut self.camera, &resp, rect, ui);

            let list = self.session.drawlist();
            paint(ui.painter_at(rect), rect, &self.camera, &list);
        });
    }
}

/// Pan on drag, zoom about the cursor on scroll.
fn handle_pan_zoom(camera: &mut Camera, resp: &egui::Response, rect: egui::Rect, ui: &egui::Ui) {
    if resp.dragged() {
        let d = resp.drag_delta();
        camera.center.x -= d.x as f64 / camera.scale;
        camera.center.y += d.y as f64 / camera.scale;
    }
    let scroll = ui.input(|i| i.raw_scroll_delta.y) as f64;
    if scroll != 0.0 {
        if let Some(pos) = resp.hover_pos() {
            let (cx, cy) = ((pos.x - rect.min.x) as f64, (pos.y - rect.min.y) as f64);
            let anchor = camera.screen_to_world(cx, cy);
            let factor = 1.0015_f64.powf(scroll);
            camera.scale = (camera.scale * factor).clamp(1e-6, 1e9);
            let (w, h) = camera.viewport;
            camera.center = Point2::new(
                anchor.x - (cx - w * 0.5) / camera.scale,
                anchor.y - (h * 0.5 - cy) / camera.scale,
            );
        }
    }
}

/// Paint the draw list into `rect` with egui's 2-D painter.
fn paint(painter: egui::Painter, rect: egui::Rect, camera: &Camera, list: &DrawList) {
    let to_screen = |p: Point2| {
        let (sx, sy) = camera.world_to_screen(p);
        egui::pos2(rect.min.x + sx as f32, rect.min.y + sy as f32)
    };

    for cmd in &list.commands {
        let color = layer_color(cmd.layer());
        let stroke = egui::Stroke::new(1.0_f32, color);
        match cmd {
            DrawCommand::Polyline { points, closed, .. } => {
                for pair in points.windows(2) {
                    painter.line_segment([to_screen(pair[0]), to_screen(pair[1])], stroke);
                }
                if *closed && points.len() > 2 {
                    painter.line_segment(
                        [to_screen(points[points.len() - 1]), to_screen(points[0])],
                        stroke,
                    );
                }
            }
            DrawCommand::Circle { center, radius, .. } => {
                painter.circle_stroke(to_screen(*center), (*radius * camera.scale) as f32, stroke);
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                ..
            } => {
                let mut prev: Option<egui::Pos2> = None;
                let sweep = normalize_sweep(*start_angle, *end_angle);
                let steps = ((sweep / 360.0 * 48.0).ceil() as usize).max(2);
                for i in 0..=steps {
                    let a = (*start_angle + sweep * i as f64 / steps as f64).to_radians();
                    let p = to_screen(Point2::new(
                        center.x + radius * a.cos(),
                        center.y + radius * a.sin(),
                    ));
                    if let Some(q) = prev {
                        painter.line_segment([q, p], stroke);
                    }
                    prev = Some(p);
                }
            }
            DrawCommand::Text {
                position, value, ..
            } => {
                painter.text(
                    to_screen(*position),
                    egui::Align2::LEFT_BOTTOM,
                    value,
                    egui::FontId::proportional(11.0),
                    color,
                );
            }
            DrawCommand::Marker { position, .. } => {
                let c = to_screen(*position);
                let d = 4.0;
                painter.line_segment([c + egui::vec2(-d, 0.0), c + egui::vec2(d, 0.0)], stroke);
                painter.line_segment([c + egui::vec2(0.0, -d), c + egui::vec2(0.0, d)], stroke);
            }
        }
    }
}

fn normalize_sweep(start: f64, end: f64) -> f64 {
    let mut d = end - start;
    while d <= 0.0 {
        d += 360.0;
    }
    d
}

/// Entities per layer, sorted, for the layers panel.
fn layer_summary(doc: &Document) -> Vec<(String, usize)> {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in doc.entities() {
        *counts.entry(e.layer.clone()).or_default() += 1;
    }
    counts.into_iter().collect()
}

/// A colour for a layer — light hues on the dark viewport, matching the canvas.
fn layer_color(layer: &str) -> egui::Color32 {
    let u = layer.to_ascii_uppercase();
    if u.starts_with("A-WALL") {
        egui::Color32::from_rgb(235, 235, 240)
    } else if u.starts_with("A-ROOM") {
        egui::Color32::from_rgb(90, 184, 230)
    } else if u.starts_with("A-DOOR") || u.starts_with("A-GLAZ") {
        egui::Color32::from_rgb(115, 210, 140)
    } else if u.starts_with("S-") {
        egui::Color32::from_rgb(235, 115, 90)
    } else if u.starts_with("A-ANNO") || u.starts_with("A-DIMS") || u.starts_with("A-DETL") {
        egui::Color32::from_rgb(225, 175, 85)
    } else {
        egui::Color32::from_rgb(160, 165, 180)
    }
}
