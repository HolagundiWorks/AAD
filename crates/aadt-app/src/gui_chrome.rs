//! AutoCAD-like chrome with Windows 11 / Fluent design principles:
//! calm surfaces, clear peer navigation, familiar commanding patterns.
//! See [`super::theme`].

use aadt_standards::{aci_to_srgb, LayerSet};
use eframe::egui;

use super::icons::{icon_btn, icon_btn_compact, icon_btn_nav, Icon};
use super::theme::{self, color, manager_title, nav_tabs, space};
use super::{layer_stroke, tool_for, AadtApp, Tool};

#[allow(dead_code)]
pub fn status_toggle(ui: &mut egui::Ui, label: &str, on: bool, tip: &str) -> egui::Response {
    let fill = if on {
        color::ACCENT_SOFT
    } else {
        color::CONTROL
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(color::TEXT))
            .fill(fill)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if on { color::ACCENT } else { color::STROKE },
            ))
            .rounding(0.0),
    )
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
                self.app_feed(&format!("LAYER {name}"));
                self.current_layer = name;
            }
            if icon_btn(ui, Icon::Delete, "Delete unused (PURGE)").clicked() {
                self.app_feed("PURGE LAYERS");
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
                            let fb = self.app_feed(&format!("LAYERSET {name}"));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                        }
                    }
                });
        });
        ui.separator();

        // Column headers (icons: On / Freeze / Lock / Remove)
        ui.horizontal(|ui| {
            ui.add_space(14.0); // current marker
            ui.label(egui::RichText::new("On").small().strong().color(color::TEXT_SECONDARY));
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Frz").small().strong().color(color::TEXT_SECONDARY));
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Lck").small().strong().color(color::TEXT_SECONDARY));
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Clr").small().strong().color(color::TEXT_SECONDARY));
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Name").small().strong().color(color::TEXT_SECONDARY));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new("LW").small().strong().color(color::TEXT_SECONDARY));
            });
        });
        ui.separator();

        let rows = self.layer_rows_snapshot();
        let total = rows.len();

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (name, color_aci, visible, frozen, locked, lw, _lt) in rows {
                let is_current = self.current_layer == name;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    // Current marker
                    if is_current {
                        ui.colored_label(color::ACCENT, "●");
                    } else {
                        ui.label(egui::RichText::new("·").color(color::STROKE));
                    }
                    // On / Off
                    let eye = if visible { Icon::Eye } else { Icon::EyeOff };
                    if icon_btn_compact(ui, eye, "On / Off", !visible).clicked() {
                        let cmd = if visible { "OFF" } else { "ON" };
                        self.app_feed(&format!("LAYER {name} {cmd}"));
                    }
                    // Freeze
                    if icon_btn_compact(ui, Icon::Freeze, "Freeze / Thaw", frozen).clicked() {
                        let cmd = if frozen { "THAW" } else { "FREEZE" };
                        self.app_feed(&format!("LAYER {name} {cmd}"));
                    }
                    // Lock
                    let lock_icon = if locked { Icon::Lock } else { Icon::Unlock };
                    if icon_btn_compact(ui, lock_icon, "Lock / Unlock", locked).clicked() {
                        let cmd = if locked { "UNLOCK" } else { "LOCK" };
                        self.app_feed(&format!("LAYER {name} {cmd}"));
                    }
                    // Color swatch (flat)
                    let (r, g, b) = aci_to_srgb(color_aci);
                    let (sw, _) =
                        ui.allocate_painter(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        sw.rect,
                        0.0,
                        egui::Color32::from_rgb(r, g, b),
                    );
                    ui.painter().rect_stroke(
                        sw.rect,
                        0.0,
                        egui::Stroke::new(1.0_f32, color::STROKE),
                    );
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
                        if name != "0"
                            && icon_btn_compact(ui, Icon::Delete, "Remove unused layer", false)
                                .clicked()
                        {
                            let fb = self.app_feed(&format!("LAYER {name} REMOVE"));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                            if self.current_layer == name {
                                self.current_layer = "0".into();
                            }
                        }
                        ui.small(format!("{lw:.2}"));
                    });
                });
            }
        });
        ui.separator();
        ui.small(format!("All: {total} layers displayed"));
    }

    pub(super) fn ui_blocks_manager(&mut self, ui: &mut egui::Ui) {
        self.ensure_blocks_cache();
        ui.label(egui::RichText::new("Drawing blocks").strong());
        if ui.button("Create from selection (BLOCK)").clicked() {
            let collect = self.selection.is_empty();
            self.tool = Tool::MakeBlock {
                erase: false,
                collecting: collect,
                name: None,
            };
            self.note = if collect {
                "BLOCK — select objects (Space when done)".into()
            } else {
                "BLOCK — type block name and Space".into()
            };
        }
        if ui.button("Create & erase sources (WBLOCK)").clicked() {
            let collect = self.selection.is_empty();
            self.tool = Tool::MakeBlock {
                erase: true,
                collecting: collect,
                name: None,
            };
            self.note = if collect {
                "WBLOCK — select objects (Space when done)".into()
            } else {
                "WBLOCK — type block name and Space".into()
            };
        }
        ui.separator();
        let blocks = self.blocks_cache.clone();
        if blocks.is_empty() {
            ui.small("(no block definitions in drawing)");
        } else {
            egui::ScrollArea::vertical()
                .max_height(220.0)
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
                        if let Some(block) = self.session.document().block(name) {
                            if !block.params.is_empty() {
                                ui.small(format!(
                                    "  params: {}",
                                    block
                                        .params
                                        .iter()
                                        .map(|p| p.name.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ));
                            }
                            let vis: Vec<_> =
                                block.visibilities.iter().map(|v| v.name.as_str()).collect();
                            if vis.len() > 1 {
                                ui.small(format!("  vis: {}", vis.join(", ")));
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.small("Add param:");
                            let key_id = egui::Id::new(("bparam_key", name.as_str()));
                            let def_id = egui::Id::new(("bparam_def", name.as_str()));
                            let mut key = ui.ctx().data_mut(|d| {
                                d.get_temp::<String>(key_id).unwrap_or_default()
                            });
                            let mut def = ui.ctx().data_mut(|d| {
                                d.get_temp::<String>(def_id).unwrap_or_default()
                            });
                            ui.add(
                                egui::TextEdit::singleline(&mut key)
                                    .desired_width(60.0)
                                    .hint_text("WNAME"),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut def)
                                    .desired_width(40.0)
                                    .hint_text("def"),
                            );
                            if ui.small_button("+").clicked() && !key.is_empty() {
                                let fb = self
                                    .session
                                    .feed(&format!("BPARAM {name} {key} {def}"));
                                if let Some(m) = fb.message() {
                                    self.note = m.to_string();
                                }
                                key.clear();
                                def.clear();
                                self.blocks_cache.clear();
                                self.blocks_cache_rev = None;
                            }
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(key_id, key);
                                d.insert_temp(def_id, def);
                            });
                        });
                    }
                });
        }
        // Selected INSERT attribute editor
        if let Some(id) = self.selection.first().copied() {
            let insert_info = self.session.document().get(id).and_then(|e| {
                match &e.geometry {
                    aadt_model::Geometry::Insert(ins) => {
                        let attrs: Vec<(String, String)> = e
                            .attributes
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        let states = self
                            .session
                            .document()
                            .block(&ins.name)
                            .map(|b| {
                                b.visibilities
                                    .iter()
                                    .map(|v| v.name.clone())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        Some((ins.name.clone(), attrs, states))
                    }
                    _ => None,
                }
            });
            if let Some((ins_name, attrs, states)) = insert_info {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("INSERT {ins_name} (#{})", id.0)).strong(),
                );
                for (k, v) in &attrs {
                    ui.horizontal(|ui| {
                        ui.small(k);
                        let mut edit = v.clone();
                        if ui
                            .add(egui::TextEdit::singleline(&mut edit).desired_width(80.0))
                            .changed()
                        {
                            let fb = self
                                .session
                                .feed(&format!("EATTEDIT {} {} {}", id.0, k, edit));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                        }
                    });
                }
                if states.len() > 1 {
                    ui.horizontal(|ui| {
                        ui.small("Visibility:");
                        for st in &states {
                            if ui.small_button(st).clicked() {
                                let fb = self.session.feed(&format!("BSETVIS {} {}", id.0, st));
                                if let Some(m) = fb.message() {
                                    self.note = m.to_string();
                                }
                            }
                        }
                    });
                }
            }
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
        let live = {
            let v = self.session.document().properties().takeoff_live.trim();
            v.is_empty() || v == "1" || v.eq_ignore_ascii_case("on")
        };
        ui.horizontal(|ui| {
            if live {
                ui.colored_label(color::ACCENT, "Live");
            } else {
                ui.label("Manual");
            }
            ui.label(
                egui::RichText::new(format!("rev {}", self.session.document().revision()))
                    .small()
                    .weak(),
            );
        });
        let t = self.session.takeoff();
        ui.label(format!("Walls: {}", t.walls.len()));
        ui.label(format!("Rooms: {}", t.rooms.len()));
        ui.label(format!("Wall length: {:.3}", t.total_wall_length()));
        ui.label(format!("Net wall area: {:.3}", t.total_wall_net_area()));
        ui.label(format!("Room area: {:.3}", t.total_room_area()));
        ui.separator();
        if ui.button("Run TAKEOFF").clicked() {
            self.app_feed("TAKEOFF");
            if let Some(fb) = self.session.transcript().last() {
                if let Some(msg) = fb.message() {
                    self.note = msg.to_string();
                }
            }
        }
        if ui.button("Publish takeoff").clicked() {
            let sink = self.takeoff_sink_spec.clone();
            if sink.is_empty() || sink == "none" {
                self.app_feed("TAKEOFF PUBLISH");
            } else {
                self.app_feed(&format!("TAKEOFF PUBLISH {sink}"));
            }
            if let Some(fb) = self.session.transcript().last() {
                if let Some(msg) = fb.message() {
                    self.note = msg.to_string();
                }
            }
        }
        if ui.button("Place SCHEDULE").clicked() {
            self.app_feed("SCHEDULE 0 0");
        }
        if ui.button("Refresh SCHEDULE").clicked() {
            self.app_feed("SCHEDULE REFRESH");
            if let Some(fb) = self.session.transcript().last() {
                if let Some(msg) = fb.message() {
                    self.note = msg.to_string();
                }
            }
        }
        ui.separator();
        ui.label(egui::RichText::new("Publish sink").small().strong());
        ui.add(
            egui::TextEdit::singleline(&mut self.takeoff_sink_spec)
                .desired_width(f32::INFINITY)
                .hint_text("none | csv:C:\\out | json:… | postgres:out.sql"),
        );
        if ui.small_button("Save sink preference").clicked() {
            self.persist_prefs();
            self.note = "Takeoff sink preference saved".into();
        }
    }

    pub(super) fn ui_history_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("One file, many versions — no RevA / RevB copies.")
                .small()
                .weak(),
        );
        ui.horizontal(|ui| {
            if ui.button("Save version…").clicked() {
                self.history_label = if self.history_label.trim().is_empty() {
                    "Named version".into()
                } else {
                    self.history_label.clone()
                };
                self.app_feed(&format!("HISTORY SAVE {}", self.history_label.replace(' ', "_")));
                if let Some(fb) = self.session.transcript().last() {
                    if let Some(msg) = fb.message() {
                        self.note = msg.to_string();
                    }
                }
            }
            if ui.button("Refresh list").clicked() {
                self.app_feed("HISTORY LIST");
                if let Some(fb) = self.session.transcript().last() {
                    if let Some(msg) = fb.message() {
                        self.note = msg.to_string();
                    }
                }
            }
            if ui.small_button("Purge autos").clicked() {
                self.app_feed("HISTORY PURGE");
            }
        });
        ui.horizontal(|ui| {
            ui.label("Label");
            ui.add(
                egui::TextEdit::singleline(&mut self.history_label)
                    .desired_width(160.0)
                    .hint_text("Issued for construction"),
            );
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Prefer store list when path is .aadt
            if let Some(path) = &self.file_path {
                if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("aadt"))
                    .unwrap_or(false)
                {
                    match aadt_store::list_checkpoints(path) {
                        Ok(list) => {
                            if list.is_empty() {
                                ui.label("No checkpoints yet — SAVE the drawing.");
                            }
                            for c in list.iter().take(40) {
                                ui.horizontal(|ui| {
                                    ui.label(format!(
                                        "#{} [{}] {}",
                                        c.id, c.kind, c.label
                                    ));
                                    if ui.small_button("Restore").clicked() {
                                        self.app_feed(&format!("HISTORY RESTORE {}", c.id));
                                        self.fitted = false;
                                        if let Some(fb) = self.session.transcript().last() {
                                            if let Some(msg) = fb.message() {
                                                self.note = msg.to_string();
                                            }
                                        }
                                    }
                                });
                                ui.label(
                                    egui::RichText::new(format!(
                                        "  {} · rev {}",
                                        c.change_summary, c.doc_revision
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                        }
                        Err(e) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(160, 60, 60),
                                format!("History: {e}"),
                            );
                        }
                    }
                } else {
                    ui.label("Save as .aadt to enable the time machine.");
                }
            } else {
                ui.label("OPEN or SAVE a .aadt drawing to use History.");
            }
        });
    }

    /// Top quick-access bar — familiar File / Edit / Layer / Plot commanding.
    pub(super) fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("qat")
            .exact_height(44.0)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_BASE)
                    .inner_margin(egui::Margin::symmetric(space::SM, space::XS))
                    .stroke(egui::Stroke::new(1.0_f32, color::BORDER)),
            )
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = space::XS;
                // File
                if icon_btn(ui, Icon::New, "New drawing (Ctrl+N)").clicked() {
                    self.new_drawing();
                }
                if icon_btn(ui, Icon::Open, "Open… (Ctrl+O)").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("AADT / DXF", &["aadt", "dxf"])
                        .pick_file()
                    {
                        let p = path.display().to_string();
                        self.app_feed(&format!("OPEN {p}"));
                        self.fitted = false;
                        self.selection.clear();
                    }
                }
                if icon_btn(ui, Icon::Save, "Save (Ctrl+S)").clicked() {
                    self.quick_save();
                }
                ui.add_space(space::SM);
                ui.separator();
                ui.add_space(space::SM);
                // Edit
                if icon_btn(ui, Icon::Undo, "Undo (Ctrl+Z)").clicked() {
                    self.app_feed("UNDO");
                }
                if icon_btn(ui, Icon::Redo, "Redo (Ctrl+Y)").clicked() {
                    self.app_feed("REDO");
                }
                ui.add_space(space::SM);
                ui.separator();
                ui.add_space(space::SM);
                // Layer (context for drafting)
                let (sw, _) = layer_stroke(
                    self.session.document(),
                    &self.current_layer,
                    &self.camera,
                    true,
                );
                let (r, painter) =
                    ui.allocate_painter(egui::vec2(16.0, 16.0), egui::Sense::hover());
                painter.rect_filled(r.rect, 0.0, sw);
                painter.rect_stroke(r.rect, 0.0, egui::Stroke::new(1.0_f32, color::STROKE));
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
                ui.label(
                    egui::RichText::new("ByLayer")
                        .small()
                        .color(color::TEXT_SECONDARY),
                );
                ui.add_space(space::SM);
                ui.separator();
                ui.add_space(space::SM);
                if icon_btn(ui, Icon::Plot, "Plot to PDF… (Ctrl+P)").clicked() {
                    self.seed_plot_dialog();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("AADT")
                            .strong()
                            .color(color::TEXT),
                    );
                });
            });
        });
    }

    pub(super) fn open_drawing_properties(&mut self) {
        self.show_drawing_props = true;
        self.props_edit = self.session.document().properties().clone();
        self.note = "Drawing Property Manager".into();
    }

    pub(super) fn draw_drawing_props_panel(&mut self, ctx: &egui::Context) {
        if !self.show_drawing_props {
            return;
        }
        egui::TopBottomPanel::bottom("drawing_props")
            .default_height(280.0)
            .min_height(160.0)
            .resizable(true)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_CARD)
                    .inner_margin(egui::Margin::same(space::MD))
                    .stroke(egui::Stroke::new(1.0_f32, color::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Drawing properties")
                            .heading()
                            .color(color::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_btn(ui, Icon::Dismiss, "Close drawing properties").clicked() {
                            self.show_drawing_props = false;
                        }
                    });
                });
                ui.add_space(space::XS);
                ui.separator();
                ui.add_space(space::SM);
                self.ui_drawing_properties(ui);
            });
    }

    pub(super) fn draw_bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(40.0)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_BASE)
                    .inner_margin(egui::Margin::symmetric(space::SM, space::XS))
                    .stroke(egui::Stroke::new(1.0_f32, color::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Active drawing units (left of command line).
                    let units = aadt_model::primary_from_props(
                        &self.session.document().properties().units,
                    );
                    let units_chip = ui.add(
                        egui::Button::new(
                            egui::RichText::new(units.as_token())
                                .strong()
                                .color(color::ACCENT),
                        )
                        .fill(color::ACCENT_SOFT)
                        .stroke(egui::Stroke::new(1.0_f32, color::ACCENT))
                        .rounding(0.0),
                    );
                    let _ = units_chip.on_hover_text(units.label());
                    if icon_btn_nav(
                        ui,
                        Icon::Props,
                        "Drawing properties (PR)",
                        self.show_drawing_props,
                    )
                    .clicked()
                    {
                        if self.show_drawing_props {
                            self.show_drawing_props = false;
                        } else {
                            self.open_drawing_properties();
                        }
                    }
                    ui.separator();
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.command)
                            .desired_width((ui.available_width() - 420.0).max(100.0))
                            .hint_text("Type a command — Space runs · F1 help"),
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

                    // Completions only while typing a token (max 6).
                    let token = self.command.trim().to_string();
                    if !token.is_empty() && !token.contains(' ') {
                        let matches = self.session.complete(&token);
                        for m in matches.iter().take(6) {
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
                    }

                    if let Some(prompt) = self.tool_prompt() {
                        ui.colored_label(color::ACCENT, prompt);
                    } else if !self.note.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(140, 100, 20),
                            &self.note,
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let n = self.session.document().len();
                        ui.label(
                            egui::RichText::new(format!("{n} ents"))
                                .small()
                                .color(color::TEXT_SECONDARY),
                        );
                        if self.dirty {
                            ui.colored_label(egui::Color32::from_rgb(160, 80, 20), "MOD");
                        }
                        if let Some(p) = &self.file_path {
                            let name = p
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("drawing");
                            let short = if name.len() > 18 {
                                format!("…{}", &name[name.len() - 16..])
                            } else {
                                name.to_string()
                            };
                            ui.label(
                                egui::RichText::new(short)
                                    .small()
                                    .color(color::TEXT_SECONDARY),
                            );
                        }
                        for (on, label) in [
                            (self.polar, "POLAR"),
                            (self.osnap, "OSNAP"),
                            (self.ortho, "ORTHO"),
                        ] {
                            if on {
                                ui.label(
                                    egui::RichText::new(label)
                                        .small()
                                        .strong()
                                        .color(color::ACCENT),
                                );
                            }
                        }
                        if let Some(c) = self.cursor {
                            ui.monospace(format!("{:.2},{:.2}", c.x, c.y));
                        }
                        egui::ComboBox::from_id_salt("opts_menu")
                            .selected_text("Options")
                            .width(80.0)
                            .show_ui(ui, |ui| {
                                if ui.checkbox(&mut self.ortho, "Ortho (F8)").changed() {
                                    self.persist_prefs();
                                }
                                if ui.checkbox(&mut self.osnap, "Osnap (F3)").changed() {
                                    self.snap_hit = None;
                                    self.persist_prefs();
                                }
                                if ui.checkbox(&mut self.polar, "Polar (F10)").changed() {
                                    self.persist_prefs();
                                }
                                if ui.checkbox(&mut self.show_lweights, "Lineweights").changed()
                                {
                                    self.persist_prefs();
                                }
                                if ui.button("Keyboard help (F1)").clicked() {
                                    self.show_shortcuts = true;
                                }
                            });
                    });
                });
            });
    }

    pub(super) fn draw_right_panel(&mut self, ctx: &egui::Context) {
        if !self.show_layer_mgr {
            // Collapsed nav rail — left-nav pattern for peer destinations.
            egui::SidePanel::right("layer_mgr_collapsed")
                .exact_width(48.0)
                .resizable(false)
                .frame(
                    egui::Frame::none()
                        .fill(color::LAYER_BASE)
                        .stroke(egui::Stroke::new(1.0_f32, color::BORDER)),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(space::SM);
                        if icon_btn_nav(ui, Icon::ExpandPanel, "Expand manager", false).clicked()
                        {
                            self.show_layer_mgr = true;
                            self.persist_prefs();
                        }
                        ui.add_space(space::SM);
                        ui.separator();
                        ui.add_space(space::XS);
                        for dest in theme::manager_nav() {
                            if icon_btn_nav(
                                ui,
                                dest.icon,
                                dest.tip,
                                self.right_tab == dest.tab,
                            )
                            .clicked()
                            {
                                self.show_layer_mgr = true;
                                self.right_tab = dest.tab;
                                if matches!(dest.tab, 6 | 7) {
                                    self.props_edit =
                                        self.session.document().properties().clone();
                                }
                                self.persist_prefs();
                            }
                        }
                    });
                });
            return;
        }
        egui::SidePanel::right("layer_mgr")
            .default_width(self.right_panel_width)
            .min_width(280.0)
            .resizable(true)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_CARD)
                    .inner_margin(egui::Margin::same(space::MD))
                    .stroke(egui::Stroke::new(1.0_f32, color::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(manager_title(self.right_tab))
                            .heading()
                            .color(color::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_btn(ui, Icon::Dismiss, "Close panel").clicked() {
                            self.show_layer_mgr = false;
                            self.persist_prefs();
                        }
                        if icon_btn(ui, Icon::CollapsePanel, "Collapse to rail").clicked() {
                            self.show_layer_mgr = false;
                            self.persist_prefs();
                        }
                    });
                });
                ui.add_space(space::XS);
                if nav_tabs(ui, &mut self.right_tab) && matches!(self.right_tab, 6 | 7) {
                    self.props_edit = self.session.document().properties().clone();
                }
                ui.add_space(space::SM);
                ui.separator();
                ui.add_space(space::SM);
                match self.right_tab {
                    0 => self.ui_layer_manager(ui),
                    1 => self.ui_blocks_manager(ui),
                    2 => self.ui_takeoff_panel(ui),
                    3 => self.ui_hatch_manager(ui),
                    4 => self.ui_annotate_panel(ui),
                    5 => self.ui_templates_panel(ui),
                    6 => self.ui_units_manager(ui),
                    7 => self.ui_dimension_manager(ui),
                    _ => self.ui_history_panel(ui),
                }
                let w = ui.max_rect().width();
                if (w - self.right_panel_width).abs() > 2.0 {
                    self.right_panel_width = w;
                    self.persist_prefs();
                }
            });
    }

    pub(super) fn ui_templates_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("Nameplate template").strong());
            ui.label(
                egui::RichText::new(
                    "Draw on TMPL_BORDER / TMPL_VIEW / TMPL_TEXT, make a block, convert it, then INSERT. Text with {title} etc. fills from Drawing Properties.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            if ui.button("Ensure template layers").clicked() {
                let fb = self.app_feed("TEMPLATE LAYERS");
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
            }
            ui.label(
                egui::RichText::new("TMPL_BORDER — border · TMPL_VIEW — drawing area · TMPL_TEXT — nameplate")
                    .small()
                    .weak(),
            );
            ui.separator();
            let nameplate = self
                .session
                .document()
                .properties()
                .nameplate_block
                .clone();
            ui.label(format!(
                "Nameplate block: {}",
                if nameplate.is_empty() {
                    "(none)"
                } else {
                    nameplate.as_str()
                }
            ));
            ui.horizontal(|ui| {
                ui.label("Block:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.insert_block).desired_width(120.0),
                );
                if ui.button("Convert to template").clicked() {
                    let name = self.insert_block.trim().to_string();
                    if name.is_empty() {
                        self.note = "Enter a block name first".into();
                    } else {
                        let fb = self.app_feed(&format!("TEMPLATE FROMBLOCK {name}"));
                        if let Some(m) = fb.message() {
                            self.note = m.to_string();
                        }
                    }
                }
            });
            if !nameplate.is_empty() && ui.button(format!("INSERT {nameplate}")).clicked() {
                self.insert_block = nameplate.clone();
                self.tool = Tool::Insert {
                    name: nameplate.clone(),
                };
                self.note = format!("INSERT {nameplate} — click to place");
            }
            ui.separator();
            ui.label(egui::RichText::new("Property fields (place as TEXT)").strong());
            ui.label(
                egui::RichText::new("Sets layer TMPL_TEXT, then click insertion point and Space.")
                    .small()
                    .weak(),
            );
            ui.horizontal_wrapped(|ui| {
                for token in [
                    "title",
                    "project",
                    "client",
                    "drawing_number",
                    "sheet_number",
                    "scale",
                    "date",
                    "revision",
                    "firm_name",
                    "drawn_by",
                    "checked_by",
                    "units",
                ] {
                    if ui.small_button(format!("{{{token}}}")).clicked() {
                        self.current_layer = "TMPL_TEXT".into();
                        self.dyn_input = format!("{{{token}}}");
                        self.tool = Tool::Text { origin: None };
                        self.note = format!(
                            "TEXT {{{token}}} — click point, then Space (layer TMPL_TEXT)"
                        );
                    }
                }
            });
            ui.separator();
            if ui.link("Open Drawing Properties").clicked() {
                self.open_drawing_properties();
            }
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
            ui.label(egui::RichText::new("Dimensions").strong());
            ui.horizontal_wrapped(|ui| {
                if ui.button("Aligned (DIM)").clicked() {
                    self.tool = Tool::Dim {
                        first: None,
                        second: None,
                    };
                    self.note = "DIM — click first point".into();
                }
                if ui.button("Continue (DIMCONTINUE)").clicked() {
                    self.tool = Tool::DimContinue;
                    self.note = "DIMCONTINUE — next definition point (Space finishes)".into();
                }
                if ui.button("Quick Dim (QDIM)").clicked() {
                    if self.selection.is_empty() {
                        self.note = "QDIM — select objects first, then pick dim-line side".into();
                    }
                    self.tool = Tool::QDim;
                    if !self.selection.is_empty() {
                        self.note = "QDIM — click dim-line side".into();
                    }
                }
                if ui.button("Auto Dim").clicked() {
                    if self.selection.is_empty() {
                        self.app_feed("AUTODIM ALL");
                        self.note = "AUTODIM ALL — structure layers isolated".into();
                    } else {
                        self.sync_selection();
                        self.app_feed("AUTODIM SELECTION");
                        self.note = "AUTODIM — structure layers isolated".into();
                    }
                    if let Some(fb) = self.session.transcript().last() {
                        if let Some(msg) = fb.message() {
                            self.note = msg.to_string();
                        }
                    }
                }
                if ui.button("Radius").clicked() {
                    self.tool = Tool::DimRadius;
                    self.note = "DIMRADIUS — click a circle or arc".into();
                }
                if ui.button("Diameter").clicked() {
                    self.tool = Tool::DimDiameter;
                    self.note = "DIMDIAMETER — click a circle or arc".into();
                }
                if ui.button("Angular").clicked() {
                    self.tool = Tool::DimAngular {
                        vertex: None,
                        p1: None,
                    };
                    self.note = "DIMANGULAR — click vertex".into();
                }
            });
            if ui.small_button("Restore layers (after Auto Dim)").clicked() {
                self.app_feed("AUTODIMRESTORE");
                if let Some(fb) = self.session.transcript().last() {
                    if let Some(msg) = fb.message() {
                        self.note = msg.to_string();
                    }
                }
            }
            if ui.small_button("Open Dimension manager…").clicked() {
                self.right_tab = 7;
            }
            ui.separator();
            ui.label(egui::RichText::new("Object count").strong());
            if ui.button("Count all objects").clicked() {
                self.app_feed("COUNT ALL");
                if let Some(fb) = self.session.transcript().last() {
                    if let Some(msg) = fb.message() {
                        self.note = msg.to_string();
                    }
                }
            }
            if ui.button("Count selection types").clicked() {
                self.sync_selection();
                self.app_feed("COUNT ALL");
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
                    self.app_feed(&format!(
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
            ui.label(
                egui::RichText::new(
                    "These fields also fill {title}, {project}, … text in nameplate templates (Templates tab).",
                )
                .small()
                .weak(),
            );
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
                    self.dirty = true;
                    self.note = "Drawing properties applied".into();
                }
                if ui.button("Reload").clicked() {
                    self.props_edit = self.session.document().properties().clone();
                }
                if ui.button("Plot PDF…").clicked() {
                    self.session
                        .set_drawing_properties(self.props_edit.clone());
                    self.dirty = true;
                    self.seed_plot_dialog();
                }
            });
        });
    }

    pub(super) fn ui_units_manager(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "Primary units control model coordinates and dimension label style.",
                )
                .small()
                .weak(),
            );
            ui.add_space(6.0);

            let primary = aadt_model::primary_from_props(&self.props_edit.units);
            ui.label(egui::RichText::new("Primary units").strong());
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        primary == aadt_model::UnitSystem::FeetInches,
                        "Feet & inches",
                    )
                    .on_hover_text("World = inches. Bare typed values are inches; use ' and \" for feet/inches.")
                    .clicked()
                {
                    self.props_edit.units = aadt_model::UnitSystem::FeetInches
                        .as_token()
                        .to_string();
                    self.props_edit.dimstyle = aadt_model::UnitSystem::FeetInches
                        .dimstyle_name()
                        .to_string();
                }
                if ui
                    .selectable_label(primary == aadt_model::UnitSystem::Meters, "Decimal meters")
                    .on_hover_text("World = metres. Bare typed values are metres.")
                    .clicked()
                {
                    self.props_edit.units =
                        aadt_model::UnitSystem::Meters.as_token().to_string();
                    self.props_edit.dimstyle =
                        aadt_model::UnitSystem::Meters.dimstyle_name().to_string();
                }
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Entry tips").strong());
            match primary {
                aadt_model::UnitSystem::FeetInches => {
                    ui.label("• `6` or `6.5` → inches (default)");
                    ui.label("• `6\"` → 6 inches");
                    ui.label("• `12'` → 12 feet");
                    ui.label("• `12'-6\"` → 12 ft 6 in");
                }
                aadt_model::UnitSystem::Meters => {
                    ui.label("• `3.5` → 3.5 metres");
                    ui.label("• `12'-6\"` still parses (converted to metres)");
                }
            }

            ui.separator();
            ui.label(egui::RichText::new("Secondary unit display").strong());
            let mut secondary = aadt_model::secondary_enabled(&self.props_edit.dim_secondary);
            let sec_label = match primary {
                aadt_model::UnitSystem::FeetInches => {
                    "Show meters in brackets on dimensions"
                }
                aadt_model::UnitSystem::Meters => {
                    "Show feet & inches in brackets on dimensions"
                }
            };
            if ui.checkbox(&mut secondary, sec_label).changed() {
                self.props_edit.dim_secondary = if secondary {
                    "1".into()
                } else {
                    "0".into()
                };
            }
            let sample = aadt_model::format_length_pair(
                match primary {
                    aadt_model::UnitSystem::FeetInches => 150.0,
                    aadt_model::UnitSystem::Meters => 3.81,
                },
                primary,
                secondary,
            );
            ui.label(format!("Sample label: {sample}"));

            ui.separator();
            ui.label(format!(
                "Dim style (from units): {}",
                if self.props_edit.dimstyle.is_empty() {
                    primary.dimstyle_name()
                } else {
                    &self.props_edit.dimstyle
                }
            ));
            if ui.small_button("Open Dimension manager…").clicked() {
                self.right_tab = 7;
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    let sys = aadt_model::primary_from_props(&self.props_edit.units);
                    if self.props_edit.dimstyle.is_empty()
                        || self.props_edit.dimstyle.eq_ignore_ascii_case("STANDARD")
                        || self
                            .props_edit
                            .dimstyle
                            .eq_ignore_ascii_case("ARCHITECTURAL")
                        || self.props_edit.dimstyle.eq_ignore_ascii_case("METRIC")
                    {
                        self.props_edit.dimstyle = sys.dimstyle_name().to_string();
                    }
                    self.session
                        .set_drawing_properties(self.props_edit.clone());
                    self.dirty = true;
                    self.note = format!(
                        "Units → {} (secondary {})",
                        sys.label(),
                        if aadt_model::secondary_enabled(&self.props_edit.dim_secondary) {
                            "on"
                        } else {
                            "off"
                        }
                    );
                }
                if ui.button("Reload").clicked() {
                    self.props_edit = self.session.document().properties().clone();
                }
            });
        });
    }

    pub(super) fn ui_dimension_manager(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let props = &self.props_edit;
            let primary = aadt_model::primary_from_props(&props.units);
            let n_dims = self.session.document().dimensions().len();
            ui.label(format!("Dimensions in drawing: {n_dims}"));
            ui.label(format!(
                "Active units: {} → style {}",
                primary.label(),
                if props.dimstyle.is_empty() {
                    primary.dimstyle_name()
                } else {
                    &props.dimstyle
                }
            ));
            ui.separator();

            ui.label(egui::RichText::new("Place").strong());
            ui.horizontal_wrapped(|ui| {
                if ui.button("Aligned (DIM)").clicked() {
                    self.tool = Tool::Dim {
                        first: None,
                        second: None,
                    };
                    self.note = "DIM — click first point".into();
                }
                if ui.button("Continue (DIMCONTINUE)").clicked() {
                    self.tool = Tool::DimContinue;
                    self.note = "DIMCONTINUE — next definition point (Space finishes)".into();
                }
                if ui.button("Quick Dim (QDIM)").clicked() {
                    if self.selection.is_empty() {
                        self.note = "QDIM — select objects first, then pick dim-line side".into();
                    }
                    self.tool = Tool::QDim;
                    if !self.selection.is_empty() {
                        self.note = "QDIM — click dim-line side".into();
                    }
                }
                if ui.button("Auto Dim").clicked() {
                    if self.selection.is_empty() {
                        self.app_feed("AUTODIM ALL");
                    } else {
                        self.sync_selection();
                        self.app_feed("AUTODIM SELECTION");
                    }
                    if let Some(fb) = self.session.transcript().last() {
                        if let Some(msg) = fb.message() {
                            self.note = msg.to_string();
                        }
                    }
                }
                if ui.button("Radius").clicked() {
                    self.tool = Tool::DimRadius;
                    self.note = "DIMRADIUS — click a circle or arc".into();
                }
                if ui.button("Diameter").clicked() {
                    self.tool = Tool::DimDiameter;
                    self.note = "DIMDIAMETER — click a circle or arc".into();
                }
                if ui.button("Angular").clicked() {
                    self.tool = Tool::DimAngular {
                        vertex: None,
                        p1: None,
                    };
                    self.note = "DIMANGULAR — click vertex".into();
                }
            });
            if ui.small_button("Restore layers (AUTODIMRESTORE)").clicked() {
                self.app_feed("AUTODIMRESTORE");
                if let Some(fb) = self.session.transcript().last() {
                    if let Some(msg) = fb.message() {
                        self.note = msg.to_string();
                    }
                }
            }

            ui.separator();
            ui.label(egui::RichText::new("Dimension style").strong());
            prop_field(ui, "Style name", &mut self.props_edit.dimstyle);
            prop_field(ui, "Text height", &mut self.props_edit.dim_text_height);
            prop_field(ui, "Overall scale", &mut self.props_edit.dim_scale);

            let mut secondary = aadt_model::secondary_enabled(&self.props_edit.dim_secondary);
            let sec_hint = match aadt_model::primary_from_props(&self.props_edit.units) {
                aadt_model::UnitSystem::FeetInches => "Secondary: meters",
                aadt_model::UnitSystem::Meters => "Secondary: feet & inches",
            };
            if ui.checkbox(&mut secondary, sec_hint).changed() {
                self.props_edit.dim_secondary = if secondary {
                    "1".into()
                } else {
                    "0".into()
                };
            }

            let preview = aadt_model::format_dimension_label(
                aadt_model::DimensionKind::Aligned,
                match aadt_model::primary_from_props(&self.props_edit.units) {
                    aadt_model::UnitSystem::FeetInches => 150.0,
                    aadt_model::UnitSystem::Meters => 3.81,
                },
                "",
                &self.props_edit.units,
                &self.props_edit.dim_secondary,
            );
            ui.label(format!("Label preview: {preview}"));

            ui.separator();
            if ui.small_button("Open Units manager…").clicked() {
                self.right_tab = 6;
            }

            ui.horizontal(|ui| {
                if ui.button("Apply style").clicked() {
                    let sys = aadt_model::primary_from_props(&self.props_edit.units);
                    if self.props_edit.dimstyle.is_empty() {
                        self.props_edit.dimstyle = sys.dimstyle_name().to_string();
                    }
                    self.session
                        .set_drawing_properties(self.props_edit.clone());
                    self.dirty = true;
                    self.note = "Dimension style applied".into();
                }
                if ui.button("Reload").clicked() {
                    self.props_edit = self.session.document().properties().clone();
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
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(color::LAYER_CARD)
                    .stroke(egui::Stroke::new(1.0_f32, color::BORDER))
                    .rounding(0.0),
            )
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
