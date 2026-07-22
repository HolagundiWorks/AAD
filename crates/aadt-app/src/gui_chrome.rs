//! AutoCAD-like chrome: quick-access bar, Layer Properties Manager, Blocks Manager,
//! and status toggles. Icons are Microsoft Fluent System Icons (MIT).

use aadt_standards::{aci_to_srgb, LayerSet};
use eframe::egui;

use super::icons::{icon_btn, Icon};
use super::{layer_stroke, tool_for, AadtApp, Tool};

pub fn status_toggle(ui: &mut egui::Ui, label: &str, on: bool, tip: &str) -> egui::Response {
    let fill = if on {
        egui::Color32::from_rgb(180, 200, 230)
    } else {
        egui::Color32::from_rgb(220, 220, 220)
    };
    ui.add(egui::Button::new(label).fill(fill))
        .on_hover_text(tip)
}

impl AadtApp {
    pub(super) fn ui_layer_manager(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.layer_filter)
                    .desired_width(ui.available_width() - 8.0)
                    .hint_text("Search for layer"),
            );
        });
        ui.horizontal(|ui| {
            if icon_btn(ui, Icon::Add, "New layer").clicked() {
                let name = format!("LAYER{}", self.session.document().layers().count() + 1);
                self.session.feed(&format!("LAYER {name}"));
                self.current_layer = name;
            }
            if icon_btn(ui, Icon::Delete, "Delete unused (PURGE)").clicked() {
                self.session.feed("PURGE LAYERS");
            }
            if icon_btn(ui, Icon::Select, "Set current").clicked() {
                self.note = format!("Current layer: {}", self.current_layer);
            }
            ui.separator();
            egui::ComboBox::from_id_salt("lset")
                .selected_text(&self.layer_set)
                .width(110.0)
                .show_ui(ui, |ui| {
                    for name in LayerSet::names() {
                        if ui
                            .selectable_label(self.layer_set == name, name)
                            .clicked()
                        {
                            self.layer_set = name.to_string();
                            let fb = self.session.feed(&format!("LAYERSET {name}"));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                        }
                    }
                });
        });
        ui.separator();

        // Column headers
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Status").small().strong());
            ui.add_space(8.0);
            ui.label(egui::RichText::new("On").small().strong());
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Freeze").small().strong());
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Color").small().strong());
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Name").small().strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new("LW").small().strong());
            });
        });
        ui.separator();

        let filter = self.layer_filter.to_ascii_uppercase();
        let mut rows: Vec<(String, u8, bool, bool, f32, String)> = self
            .session
            .document()
            .layers()
            .filter(|l| filter.is_empty() || l.name.to_ascii_uppercase().contains(&filter))
            .map(|l| {
                (
                    l.name.clone(),
                    l.color,
                    l.visible,
                    l.frozen,
                    l.lineweight_mm,
                    l.linetype.dxf_name().to_string(),
                )
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let total = rows.len();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (name, color, visible, frozen, lw, _lt) in rows {
                let is_current = self.current_layer == name;
                ui.horizontal(|ui| {
                    // Current marker
                    if is_current {
                        ui.colored_label(egui::Color32::from_rgb(40, 140, 40), "●");
                    } else {
                        ui.label(" ");
                    }
                    // On
                    let on_label = if visible { "On" } else { "Off" };
                    if ui
                        .add(egui::Button::new(on_label).min_size(egui::vec2(28.0, 18.0)))
                        .on_hover_text("On / Off")
                        .clicked()
                    {
                        let cmd = if visible { "OFF" } else { "ON" };
                        self.session.feed(&format!("LAYER {name} {cmd}"));
                    }
                    // Freeze
                    let fr_label = if frozen { "Frz" } else { "Thw" };
                    if ui
                        .add(egui::Button::new(fr_label).min_size(egui::vec2(28.0, 18.0)))
                        .on_hover_text("Freeze / Thaw")
                        .clicked()
                    {
                        let cmd = if frozen { "THAW" } else { "FREEZE" };
                        self.session.feed(&format!("LAYER {name} {cmd}"));
                    }
                    // Color swatch
                    let (r, g, b) = aci_to_srgb(color);
                    let (sw, _) =
                        ui.allocate_painter(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(sw.rect, 2.0, egui::Color32::from_rgb(r, g, b));
                    // Name — click sets current
                    let name_resp = ui.selectable_label(is_current, &name);
                    if name_resp.clicked() {
                        self.current_layer = name.clone();
                    }
                    if name_resp.double_clicked() {
                        self.current_layer = name.clone();
                        self.note = format!("Current layer set to {name}");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(format!("{lw:.2}"));
                    });
                });
            }
        });
        ui.separator();
        ui.small(format!("All: {total} layers displayed"));
    }

    pub(super) fn ui_blocks_manager(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Drawing blocks").strong());
        let blocks: Vec<(String, usize)> = self
            .session
            .document()
            .blocks()
            .into_iter()
            .map(|b| (b.name.clone(), b.entities.len()))
            .collect();
        if blocks.is_empty() {
            ui.small("(no block definitions in drawing)");
        } else {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for (name, n) in &blocks {
                        ui.horizontal(|ui| {
                            if ui.button(format!("Insert {name}")).clicked() {
                                self.insert_block = name.clone();
                                self.tool = Tool::Insert {
                                    name: name.clone(),
                                };
                                self.note =
                                    format!("INSERT {name} — click to place (Esc finishes)");
                            }
                            ui.small(format!("{n} ents"));
                        });
                    }
                });
        }
        ui.separator();
        ui.label(egui::RichText::new("Block library (DXF/SVG)").strong());
        let root = aadt_library::default_library_root();
        match aadt_library::list_blocks(&root) {
            Ok(lib) if lib.is_empty() => {
                ui.small(format!("(empty — {})", root.join("blocks").display()));
            }
            Ok(lib) => {
                for b in lib {
                    ui.horizontal(|ui| {
                        if ui.button(format!("Import {}", b.name)).clicked() {
                            let p = b.path.display().to_string();
                            let fb = self
                                .session
                                .feed(&format!("BLOCKLIB import {p} {}", b.name));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                            self.insert_block = b.name.clone();
                            self.tool = Tool::Insert {
                                name: b.name.clone(),
                            };
                        }
                    });
                }
            }
            Err(e) => {
                ui.label(e.to_string());
            }
        }
        if !self.insert_block.is_empty() {
            ui.separator();
            ui.label(format!("Active INSERT: {}", self.insert_block));
        }
    }

    pub(super) fn ui_takeoff_panel(&mut self, ui: &mut egui::Ui) {
        let t = self.session.takeoff();
        ui.label(format!("Walls: {}", t.walls.len()));
        ui.label(format!("Rooms: {}", t.rooms.len()));
        ui.label(format!("Wall length: {:.3}", t.total_wall_length()));
        ui.label(format!("Net wall area: {:.3}", t.total_wall_net_area()));
        ui.label(format!("Room area: {:.3}", t.total_room_area()));
        ui.separator();
        if ui.button("Run TAKEOFF").clicked() {
            self.session.feed("TAKEOFF");
        }
        if ui.button("Place SCHEDULE").clicked() {
            self.session.feed("SCHEDULE 0 0");
        }
    }

    /// Top quick-access bar + layer dropdown.
    pub(super) fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("qat").exact_height(36.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if icon_btn(ui, Icon::New, "New drawing").clicked() {
                    let mut d = aadt_model::Document::new();
                    aadt_standards::apply_standard_layers(&mut d);
                    self.session.set_document(d);
                    self.selection.clear();
                    self.fitted = false;
                    self.note = "New drawing".into();
                }
                if icon_btn(ui, Icon::Open, "Open…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("AADT / DXF", &["aadt", "dxf"])
                        .pick_file()
                    {
                        let p = path.display().to_string();
                        self.session.feed(&format!("OPEN {p}"));
                        self.fitted = false;
                        self.selection.clear();
                    }
                }
                if icon_btn(ui, Icon::Save, "Save…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("AADT", &["aadt"])
                        .add_filter("DXF", &["dxf"])
                        .set_file_name("drawing.aadt")
                        .save_file()
                    {
                        let p = path.display().to_string();
                        self.session.feed(&format!("SAVE {p}"));
                    }
                }
                ui.separator();
                if icon_btn(ui, Icon::Undo, "Undo").clicked() {
                    self.session.feed("UNDO");
                }
                if icon_btn(ui, Icon::Redo, "Redo").clicked() {
                    self.session.feed("REDO");
                }
                ui.separator();
                let (sw, _) = layer_stroke(
                    self.session.document(),
                    &self.current_layer,
                    &self.camera,
                    true,
                );
                let (r, painter) =
                    ui.allocate_painter(egui::vec2(14.0, 14.0), egui::Sense::hover());
                painter.rect_filled(r.rect, 2.0, sw);
                egui::ComboBox::from_id_salt("cur_layer")
                    .selected_text(&self.current_layer)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        let mut names: Vec<String> = self
                            .session
                            .document()
                            .layers()
                            .map(|l| l.name.clone())
                            .collect();
                        names.sort();
                        for name in names {
                            let selected = self.current_layer == name;
                            if ui.selectable_label(selected, &name).clicked() {
                                self.current_layer = name;
                            }
                        }
                    });
                ui.label(egui::RichText::new("ByLayer").small().weak());
                ui.separator();
                // Panel tabs (Layers / Blocks / …) live on the right strip when collapsed.
                if icon_btn(ui, Icon::Plot, "Plot to PDF…").clicked() {
                    self.seed_plot_dialog();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("AADT").strong());
                });
            });
        });
    }

    pub(super) fn draw_bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(58.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let _ = ui.add(egui::Button::new("Model").selected(true));
                    ui.separator();
                    ui.label(egui::RichText::new("Type a command").small().weak());
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.command)
                            .desired_width((ui.available_width() - 260.0).max(120.0))
                            .hint_text("LINE, OFFSET, LAYERSET, LISP…"),
                    );
                    if self.refocus {
                        resp.request_focus();
                        self.refocus = false;
                    }
                    if self.command.ends_with(' ') {
                        let token = self.command.trim().to_string();
                        if !token.is_empty() && !token.contains(' ') {
                            let up = token.to_ascii_uppercase();
                            let interactive = matches!(up.as_str(), "E" | "ERASE")
                                || tool_for(&token, !self.selection.is_empty()).is_some();
                            if interactive {
                                self.command.clear();
                                self.last_line = Some(token.clone());
                                self.dispatch(&token);
                            }
                        }
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.submit_command_line();
                    }
                });
                ui.horizontal(|ui| {
                    if let Some(prompt) = self.tool_prompt() {
                        ui.colored_label(egui::Color32::from_rgb(30, 90, 160), prompt);
                    } else if !self.note.is_empty() {
                        ui.colored_label(egui::Color32::from_rgb(140, 100, 20), &self.note);
                    } else if let Some(fb) = self.session.transcript().last() {
                        if let Some(msg) = fb.message() {
                            if fb.is_err() {
                                ui.colored_label(egui::Color32::from_rgb(180, 40, 40), msg);
                            } else {
                                ui.monospace(msg);
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(c) = self.cursor {
                            ui.monospace(format!("{:.2}, {:.2}", c.x, c.y));
                        }
                        ui.separator();
                        if status_toggle(ui, "POLAR", self.polar, "Polar tracking (F10)").clicked()
                        {
                            self.polar = !self.polar;
                        }
                        if status_toggle(ui, "LW", self.show_lweights, "Show lineweights").clicked()
                        {
                            self.show_lweights = !self.show_lweights;
                        }
                        if status_toggle(ui, "OSNAP", self.osnap, "Object snap (F3)").clicked() {
                            self.osnap = !self.osnap;
                            self.snap_hit = None;
                        }
                        if status_toggle(ui, "ORTHO", self.ortho, "Ortho (F8)").clicked() {
                            self.ortho = !self.ortho;
                        }
                        ui.small("1:100");
                    });
                });
                let token = self.command.trim().to_string();
                if !token.is_empty() && !token.contains(' ') {
                    let matches = self.session.complete(&token);
                    if !matches.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for m in matches.iter().take(12) {
                                if ui.small_button(m).clicked() {
                                    let up = m.to_ascii_uppercase();
                                    if matches!(up.as_str(), "E" | "ERASE")
                                        || tool_for(m, !self.selection.is_empty()).is_some()
                                    {
                                        self.command.clear();
                                        self.last_line = Some(m.clone());
                                        self.dispatch(m);
                                    } else {
                                        self.command = format!("{m} ");
                                        self.refocus = true;
                                    }
                                }
                            }
                        });
                    }
                }
            });
    }

    pub(super) fn draw_right_panel(&mut self, ctx: &egui::Context) {
        if !self.show_layer_mgr {
            // Collapsed: thin strip so the manager can be re-opened.
            egui::SidePanel::right("layer_mgr_collapsed")
                .exact_width(28.0)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        if icon_btn(ui, Icon::ExpandPanel, "Expand Layer Manager").clicked() {
                            self.show_layer_mgr = true;
                        }
                        ui.add_space(6.0);
                        if icon_btn(ui, Icon::Layers, "Layers").clicked() {
                            self.show_layer_mgr = true;
                            self.right_tab = 0;
                        }
                        if icon_btn(ui, Icon::Blocks, "Blocks").clicked() {
                            self.show_layer_mgr = true;
                            self.right_tab = 1;
                        }
                        if icon_btn(ui, Icon::Hatch, "Hatches").clicked() {
                            self.show_layer_mgr = true;
                            self.right_tab = 4;
                        }
                        if icon_btn(ui, Icon::Annotate, "Annotate").clicked() {
                            self.show_layer_mgr = true;
                            self.right_tab = 5;
                        }
                        if icon_btn(ui, Icon::Ai, "AI Recognize").clicked() {
                            self.show_layer_mgr = true;
                            self.right_tab = 6;
                        }
                        if icon_btn(ui, Icon::Props, "Properties").clicked() {
                            self.show_layer_mgr = true;
                            self.right_tab = 3;
                            self.props_edit = self.session.document().properties().clone();
                        }
                    });
                });
            return;
        }
        egui::SidePanel::right("layer_mgr")
            .default_width(340.0)
            .min_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(match self.right_tab {
                        3 => "DRAWING PROPERTY MANAGER",
                        1 => "BLOCKS MANAGER",
                        2 => "TAKEOFF",
                        4 => "HATCH MANAGER",
                        5 => "ANNOTATE",
                        6 => "AI RECOGNIZE",
                        _ => "LAYER PROPERTIES MANAGER",
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_btn(ui, Icon::Dismiss, "Close panel").clicked() {
                            self.show_layer_mgr = false;
                        }
                        if icon_btn(ui, Icon::CollapsePanel, "Collapse panel").clicked() {
                            self.show_layer_mgr = false;
                        }
                    });
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(self.right_tab == 0, "Layers").clicked() {
                        self.right_tab = 0;
                    }
                    if ui.selectable_label(self.right_tab == 1, "Blocks").clicked() {
                        self.right_tab = 1;
                    }
                    if ui.selectable_label(self.right_tab == 2, "Takeoff").clicked() {
                        self.right_tab = 2;
                    }
                    if ui.selectable_label(self.right_tab == 3, "Properties").clicked() {
                        self.right_tab = 3;
                        self.props_edit = self.session.document().properties().clone();
                    }
                    if ui.selectable_label(self.right_tab == 4, "Hatches").clicked() {
                        self.right_tab = 4;
                    }
                    if ui.selectable_label(self.right_tab == 5, "Annotate").clicked() {
                        self.right_tab = 5;
                    }
                    if ui.selectable_label(self.right_tab == 6, "AI").clicked() {
                        self.right_tab = 6;
                    }
                });
                ui.separator();
                match self.right_tab {
                    0 => self.ui_layer_manager(ui),
                    1 => self.ui_blocks_manager(ui),
                    2 => self.ui_takeoff_panel(ui),
                    3 => self.ui_drawing_properties(ui),
                    4 => self.ui_hatch_manager(ui),
                    5 => self.ui_annotate_panel(ui),
                    _ => self.ui_ai_recognize_panel(ui),
                }
            });
    }

    pub(super) fn ui_ai_recognize_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("AI Recognize (walls)").strong());
            ui.label(
                egui::RichText::new(
                    "Proposes classifications only — geometry is never rewritten. Freeze locks approved types.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            if ui.button("Recognize Drawing").clicked() {
                let fb = self.session.feed("RECOGNIZE");
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Review list").clicked() {
                    let fb = self.session.feed("REVIEW");
                    if let Some(m) = fb.message() {
                        self.note = m.to_string();
                    }
                }
                if ui.button("Freeze all accepted").clicked() {
                    let fb = self.session.feed("FREEZE");
                    if let Some(m) = fb.message() {
                        self.note = m.to_string();
                    }
                }
            });
            ui.separator();
            let counts = aadt_ai::status_counts(self.session.document());
            ui.label(format!(
                "Proposed: {}  Accepted: {}  Frozen: {}  Rejected: {}",
                counts[0].1, counts[1].1, counts[2].1, counts[3].1
            ));
            let review =
                aadt_ai::list_for_review(self.session.document(), aadt_ai::DEFAULT_AUTO_ACCEPT_THRESHOLD);
            ui.label(format!("Needs review: {}", review.len()));
            ui.separator();
            ui.label(egui::RichText::new("Exceptions (low confidence / proposed)").strong());
            for it in review.iter().take(40) {
                ui.horizontal(|ui| {
                    let selected = self.selection.contains(&it.entity_id);
                    if ui
                        .selectable_label(
                            selected,
                            format!(
                                "#{} {} {:.0}% {}",
                                it.entity_id.0,
                                it.wall_type,
                                it.confidence * 100.0,
                                it.status.as_token()
                            ),
                        )
                        .clicked()
                    {
                        self.selection = vec![it.entity_id];
                        self.sync_selection();
                    }
                    if ui.small_button("Accept").clicked() {
                        self.selection = vec![it.entity_id];
                        self.sync_selection();
                        let fb = self.session.feed("ACCEPT SELECTION");
                        if let Some(m) = fb.message() {
                            self.note = m.to_string();
                        }
                    }
                    if ui.small_button("Freeze").clicked() {
                        let fb = self
                            .session
                            .feed(&format!("FREEZE {}", it.entity_id.0));
                        if let Some(m) = fb.message() {
                            self.note = m.to_string();
                        }
                    }
                });
            }
            if review.len() > 40 {
                ui.label(format!("…and {} more (run REVIEW)", review.len() - 40));
            }
            ui.separator();
            ui.label(egui::RichText::new("Override type on selection").small());
            ui.horizontal(|ui| {
                for ty in ["exterior", "partition", "shaft", "fire-rated"] {
                    if ui.small_button(ty).clicked() {
                        let fb = self.session.feed(&format!("ACCEPT SELECTION {ty}"));
                        if let Some(m) = fb.message() {
                            self.note = m.to_string();
                        }
                    }
                }
            });
        });
    }

    pub(super) fn ui_hatch_manager(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Prebuilt patterns").strong());
        ui.label(
            egui::RichText::new(format!("Active: {}", self.hatch_pattern))
                .small()
                .weak(),
        );
        ui.horizontal(|ui| {
            if ui.button("Apply Hatch (H)").clicked() {
                self.tool = Tool::Hatch;
                self.note = format!(
                    "HATCH ({}) — click a closed polyline or circle",
                    self.hatch_pattern
                );
            }
            if ui.small_button("SOLID").clicked() {
                self.hatch_pattern = "SOLID".into();
            }
        });
        ui.separator();
        let root = aadt_library::default_library_root();
        let patterns = aadt_library::list_hatch_patterns(&root)
            .unwrap_or_else(|_| aadt_library::builtin_hatch_patterns());
        egui::ScrollArea::vertical().show(ui, |ui| {
            for pat in patterns {
                let selected = self.hatch_pattern.eq_ignore_ascii_case(&pat.name);
                ui.horizontal(|ui| {
                    if ui.selectable_label(selected, &pat.name).clicked() {
                        self.hatch_pattern = pat.name.clone();
                    }
                    ui.label(egui::RichText::new(&pat.description).small().weak());
                });
            }
        });
    }

    pub(super) fn ui_annotate_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("Insert").strong());
            ui.horizontal(|ui| {
                ui.label("Table rows");
                ui.add(
                    egui::TextEdit::singleline(&mut self.table_rows).desired_width(40.0),
                );
                ui.label("cols");
                ui.add(
                    egui::TextEdit::singleline(&mut self.table_cols).desired_width(40.0),
                );
            });
            if ui.button("Insert Table…").clicked() {
                self.tool = Tool::TablePlace;
            }
            ui.separator();
            ui.label(egui::RichText::new("Leaders & marks").strong());
            if ui.button("Multileader (LE)").clicked() {
                self.tool = Tool::Mleader {
                    arrow: None,
                    landing: None,
                    text_pt: None,
                };
            }
            if ui.button("Callout / bubble").clicked() {
                self.tool = Tool::Callout {
                    arrow: None,
                    center: None,
                };
            }
            if ui.button("Section line").clicked() {
                self.tool = Tool::SectionLine { first: None };
            }
            if ui.button("Revision mark").clicked() {
                self.tool = Tool::RevMark;
            }
            if ui.button("Revision cloud").clicked() {
                self.tool = Tool::RevCloud { first: None };
            }
            ui.separator();
            ui.label(egui::RichText::new("Object count").strong());
            if ui.button("Count all objects").clicked() {
                self.session.feed("COUNT ALL");
                if let Some(fb) = self.session.transcript().last() {
                    if let Some(msg) = fb.message() {
                        self.note = msg.to_string();
                    }
                }
            }
            if ui.button("Count selection types").clicked() {
                self.sync_selection();
                self.session.feed("COUNT ALL");
            }
            ui.separator();
            ui.label(egui::RichText::new("Auto-number").strong());
            ui.horizontal(|ui| {
                ui.label("Prefix");
                ui.add(
                    egui::TextEdit::singleline(&mut self.autonum_prefix).desired_width(60.0),
                );
                ui.label("Start");
                ui.add(
                    egui::TextEdit::singleline(&mut self.autonum_start).desired_width(40.0),
                );
            });
            ui.label(
                egui::RichText::new("Select TEXT / INSERT / CIRCLE objects, then Number.")
                    .small()
                    .weak(),
            );
            if ui.button("Number selection").clicked() {
                if self.selection.is_empty() {
                    self.note = "Select objects to number first".into();
                } else {
                    self.sync_selection();
                    self.session.feed(&format!(
                        "AUTONUM SELECTION {} {}",
                        self.autonum_prefix, self.autonum_start
                    ));
                    if let Some(fb) = self.session.transcript().last() {
                        if let Some(msg) = fb.message() {
                            self.note = msg.to_string();
                            // Pull next value from drawing properties after success.
                            let props = self.session.document().properties();
                            if !props.autonum_next.is_empty() {
                                self.autonum_start = props.autonum_next.clone();
                            }
                            if !props.autonum_prefix.is_empty() {
                                self.autonum_prefix = props.autonum_prefix.clone();
                            }
                        }
                    }
                }
            }
        });
    }

    pub(super) fn ui_drawing_properties(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("Summary").strong());
            prop_field(ui, "Title", &mut self.props_edit.title);
            prop_field(ui, "Subject", &mut self.props_edit.subject);
            prop_field(ui, "Author", &mut self.props_edit.author);
            prop_field(ui, "Keywords", &mut self.props_edit.keywords);
            ui.label("Comments");
            ui.add(
                egui::TextEdit::multiline(&mut self.props_edit.comments)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3),
            );
            prop_field(ui, "Hyperlink base", &mut self.props_edit.hyperlink_base);
            ui.separator();
            ui.label(egui::RichText::new("Title block (fixed layout / per-sheet attributes)").strong());
            prop_field(ui, "Company name", &mut self.props_edit.firm_name);
            prop_field(ui, "Tagline", &mut self.props_edit.firm_tagline);
            prop_field(ui, "Office address", &mut self.props_edit.office_address);
            prop_field(ui, "Phone", &mut self.props_edit.phone);
            prop_field(ui, "Email", &mut self.props_edit.email);
            prop_field(ui, "Website", &mut self.props_edit.website);
            ui.horizontal(|ui| {
                ui.label("Logo");
                let logo = if self.props_edit.logo_path.is_empty() {
                    "(none)".to_string()
                } else {
                    std::path::Path::new(&self.props_edit.logo_path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("logo")
                        .to_string()
                };
                ui.monospace(&logo);
                if ui.button("Upload…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Logo", &["png", "jpg", "jpeg", "svg"])
                        .pick_file()
                    {
                        self.props_edit.logo_path = path.display().to_string();
                    }
                }
                if ui.small_button("Clear").clicked() {
                    self.props_edit.logo_path.clear();
                }
            });
            ui.separator();
            ui.label(egui::RichText::new("Project (shared across set)").strong());
            prop_field(ui, "Project name", &mut self.props_edit.project);
            prop_field(ui, "Client", &mut self.props_edit.client);
            prop_field(ui, "Address", &mut self.props_edit.address);
            prop_field(ui, "Project / job no.", &mut self.props_edit.project_number);
            prop_field(ui, "Architect", &mut self.props_edit.architect);
            prop_field(ui, "Area", &mut self.props_edit.area);
            ui.separator();
            ui.label(egui::RichText::new("This sheet (attributes)").strong());
            prop_field(ui, "Drawing title", &mut self.props_edit.title);
            prop_field(ui, "Drawing number (A-101)", &mut self.props_edit.drawing_number);
            prop_field(ui, "Sheet number (01 of 45)", &mut self.props_edit.sheet_number);
            prop_field(ui, "Total sheets", &mut self.props_edit.total_sheets);
            prop_field(ui, "Discipline", &mut self.props_edit.discipline);
            prop_field(ui, "Scale", &mut self.props_edit.scale);
            prop_field(ui, "Date", &mut self.props_edit.date);
            prop_field(ui, "Drawn by", &mut self.props_edit.drawn_by);
            prop_field(ui, "Checked by", &mut self.props_edit.checked_by);
            prop_field(ui, "Approved by", &mut self.props_edit.approved_by);
            prop_field(ui, "Revision", &mut self.props_edit.revision);
            prop_field(ui, "Drawing status", &mut self.props_edit.drawing_status);
            ui.label("Revision history (Rev|Date|Description|By|Checked per line)");
            ui.add(
                egui::TextEdit::multiline(&mut self.props_edit.revision_history)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .hint_text("00|22-Jul-2026|Initial Issue|VH|RK"),
            );
            ui.separator();
            ui.label(egui::RichText::new("Plot / nameplate").strong());
            ui.horizontal(|ui| {
                ui.label("Nameplate");
                let cur = aadt_docs::NameplatePreset::parse(&self.props_edit.nameplate_preset)
                    .unwrap_or(aadt_docs::NameplatePreset::Professional);
                egui::ComboBox::from_id_salt("props_nameplate")
                    .selected_text(cur.label())
                    .show_ui(ui, |ui| {
                        for p in [
                            aadt_docs::NameplatePreset::Professional,
                            aadt_docs::NameplatePreset::Compact,
                            aadt_docs::NameplatePreset::Minimal,
                        ] {
                            if ui
                                .selectable_label(cur == p, p.label())
                                .clicked()
                            {
                                self.props_edit.nameplate_preset = p.as_token().to_string();
                            }
                        }
                    });
            });
            let mut include = self.props_edit.plot_include_notes.trim() != "0";
            if ui
                .checkbox(&mut include, "Include standard notes when plotting")
                .changed()
            {
                self.props_edit.plot_include_notes =
                    if include { "1".into() } else { "0".into() };
            }
            ui.label("Standard notes (one per line)");
            ui.add(
                egui::TextEdit::multiline(&mut self.props_edit.standard_notes)
                    .desired_width(f32::INFINITY)
                    .desired_rows(5)
                    .hint_text("Saved on the drawing; edited again at plot time"),
            );
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    self.session
                        .set_drawing_properties(self.props_edit.clone());
                    self.note = "Drawing properties applied".into();
                }
                if ui.button("Reload").clicked() {
                    self.props_edit = self.session.document().properties().clone();
                }
                if ui.button("Plot PDF…").clicked() {
                    self.session
                        .set_drawing_properties(self.props_edit.clone());
                    self.seed_plot_dialog();
                }
            });
        });
    }

    pub(super) fn draw_plot_dialog(&mut self, ctx: &egui::Context) {
        if !self.plot_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Plot to PDF")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Always exports a vector PDF (no direct print).");
                ui.label(
                    "Sheet + title-block orientation follow the selection (or whole drawing) extents.",
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Drawing name");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.plot_drawing_name)
                            .desired_width(280.0)
                            .hint_text("Shown on the title plate"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Paper");
                    egui::ComboBox::from_id_salt("plot_paper")
                        .selected_text(&self.plot_paper)
                        .show_ui(ui, |ui| {
                            for name in ["A4", "A3", "A2", "A1", "A0", "ANSI-D", "ARCH-D"] {
                                if ui
                                    .selectable_label(self.plot_paper == name, name)
                                    .clicked()
                                {
                                    self.plot_paper = name.to_string();
                                }
                            }
                        });
                    let orient = aadt_docs::orientation_from_selection(
                        self.session.document(),
                        &self.selection,
                    );
                    let layout = aadt_docs::TitleBlockLayout::for_orientation(orient);
                    let label = match (orient, layout) {
                        (
                            aadt_docs::Orientation::Landscape,
                            aadt_docs::TitleBlockLayout::VerticalRight,
                        ) => {
                            if self.selection.is_empty() {
                                "Auto: Landscape · title strip right"
                            } else {
                                "Auto (selection): Landscape · title strip right"
                            }
                        }
                        (
                            aadt_docs::Orientation::Portrait,
                            aadt_docs::TitleBlockLayout::HorizontalBottom,
                        ) => {
                            if self.selection.is_empty() {
                                "Auto: Portrait · title band bottom"
                            } else {
                                "Auto (selection): Portrait · title band bottom"
                            }
                        }
                        _ => "Auto",
                    };
                    ui.label(label);
                });
                ui.horizontal(|ui| {
                    ui.label("Nameplate");
                    let cur = aadt_docs::NameplatePreset::parse(&self.plot_nameplate)
                        .unwrap_or(aadt_docs::NameplatePreset::Professional);
                    egui::ComboBox::from_id_salt("plot_nameplate")
                        .selected_text(cur.label())
                        .show_ui(ui, |ui| {
                            for p in [
                                aadt_docs::NameplatePreset::Professional,
                                aadt_docs::NameplatePreset::Compact,
                                aadt_docs::NameplatePreset::Minimal,
                            ] {
                                if ui.selectable_label(cur == p, p.label()).clicked() {
                                    self.plot_nameplate = p.as_token().to_string();
                                }
                            }
                        });
                });
                ui.separator();
                ui.checkbox(&mut self.plot_include_notes, "Include standard notes");
                ui.horizontal(|ui| {
                    ui.label("Notes");
                    if ui.button("Reload suggested").clicked() {
                        let det = aadt_detect::detect(
                            self.session.document(),
                            &aadt_detect::DetectionConfig::default(),
                        );
                        self.plot_notes = aadt_docs::notes_to_text(
                            &aadt_docs::NotesLibrary::standard().suggest(&det),
                        );
                    }
                });
                ui.add(
                    egui::TextEdit::multiline(&mut self.plot_notes)
                        .desired_width(f32::INFINITY)
                        .desired_rows(8)
                        .hint_text("One note per line — saved on the drawing"),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.plot_open = false;
                    }
                    if ui
                        .button("Export PDF…")
                        .on_hover_text("Compose sheet and save PDF")
                        .clicked()
                    {
                        self.export_plot_pdf();
                    }
                });
            });
        if !open {
            self.plot_open = false;
        }
    }

    fn export_plot_pdf(&mut self) {
        let paper = aadt_docs::PaperSize::from_name(&self.plot_paper)
            .unwrap_or(aadt_docs::PaperSize::IsoA3);
        let mut props = self.session.document().properties().clone();
        if !self.plot_drawing_name.trim().is_empty() {
            props.title = self.plot_drawing_name.trim().to_string();
        }
        props.standard_notes = self.plot_notes.clone();
        props.plot_include_notes = if self.plot_include_notes {
            "1".into()
        } else {
            "0".into()
        };
        props.nameplate_preset = self.plot_nameplate.clone();
        self.session.set_drawing_properties(props.clone());
        self.props_edit = props.clone();

        let tb = aadt_docs::TitleBlock::from_properties(&props, &self.plot_drawing_name);
        let (mut sheet_doc, sheet, composed) = aadt_docs::compose_plot_sheet_for_selection(
            self.session.document(),
            paper,
            &tb,
            &self.selection,
        );

        if self.plot_include_notes {
            let notes = aadt_docs::notes_from_text(&self.plot_notes);
            if !notes.is_empty() {
                let area = aadt_docs::drawing_area_for_preset(&sheet, tb.preset);
                let notes_style = aadt_docs::NotesStyle {
                    origin: aadt_geometry::Point2::new(area.min.x + 5.0, area.max.y - 5.0),
                    ..Default::default()
                };
                aadt_docs::place_notes(&mut sheet_doc, &notes, &notes_style);
            }
        }

        let (w, h) = sheet.extent();
        let page = aadt_pdf::PageSize::from_mm(w, h);
        let pdf = aadt_pdf::plot(&sheet_doc, page);

        let default_name = format!(
            "{}.pdf",
            sanitize_filename(if self.plot_drawing_name.trim().is_empty() {
                "drawing"
            } else {
                self.plot_drawing_name.trim()
            })
        );
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(&default_name)
            .save_file()
        {
            match std::fs::write(&path, &pdf) {
                Ok(()) => {
                    self.note = format!(
                        "Plotted PDF {} ({}) — scale {}",
                        path.display(),
                        match sheet.orientation {
                            aadt_docs::Orientation::Landscape => "landscape",
                            aadt_docs::Orientation::Portrait => "portrait",
                        },
                        composed.scale_label(1000.0)
                    );
                    self.plot_open = false;
                }
                Err(e) => {
                    self.note = format!("PDF write failed: {e}");
                }
            }
        }
    }

}

fn prop_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
    });
}

fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "drawing".into()
    } else {
        s
    }
}
