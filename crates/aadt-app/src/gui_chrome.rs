//! AutoCAD-like chrome styled to Fluent 2 foundations
//! (color, elevation, layout, shapes, typography). See [`super::theme`].

use aadt_standards::{aci_to_srgb, LayerSet};
use eframe::egui;

use super::icons::{brand_mark_widget, icon_btn, icon_btn_compact, icon_btn_nav, Icon};
use super::theme::{
    self, color, full_width_button, full_width_edit, left_manager_title, panel_space,
    panel_subtitle, panel_title, radius, right_manager_title, section_hint, section_label,
    section_label_first, space, stroke, two_col_buttons, type_scale,
};
use super::{
    block_name_for_category, library_path_for_category, place_kinds_for_category, tool_for,
    AadtApp, LayerMgrScope, Tool,
};

impl AadtApp {
    /// Sync the drafting layer shim from the active model-space category.
    pub(super) fn sync_layer_from_active_space(&mut self) {
        self.current_layer = self.session.document().active_category_layer();
    }

    /// Floor / category changes leave prior selection on another space — clear it.
    pub(super) fn on_floor_or_space_changed(&mut self) {
        self.selection.clear();
        self.sync_layer_from_active_space();
        self.layer_rows_cache = None;
    }

    pub(super) fn ui_levels_strip(&mut self, ui: &mut egui::Ui) {
        use aadt_model::SpaceCategory;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Floor").strong().small());
            let floors: Vec<(aadt_model::LevelId, String, u32)> = self
                .session
                .document()
                .floors()
                .into_iter()
                .map(|f| (f.id, f.name.clone(), f.index))
                .collect();
            let active_floor = self.session.document().active_floor_id();
            let active_name = floors
                .iter()
                .find(|(id, _, _)| Some(*id) == active_floor)
                .map(|(_, n, _)| n.as_str())
                .unwrap_or("Ground");
            egui::ComboBox::from_id_salt("floor_combo")
                .selected_text(active_name)
                .width(100.0)
                .show_ui(ui, |ui| {
                    for (id, name, idx) in &floors {
                        if ui
                            .selectable_label(Some(*id) == active_floor, format!("[{idx}] {name}"))
                            .clicked()
                        {
                            let fb = self.app_feed(&format!("FLOOR {name}"));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                            self.on_floor_or_space_changed();
                        }
                    }
                });
            if ui.small_button("+").on_hover_text("Add floor above").clicked() {
                let n = self.session.document().floors().len() + 1;
                let fb = self.app_feed(&format!("FLOORADD Level {n}"));
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
                self.on_floor_or_space_changed();
            }
            ui.separator();
            ui.label(egui::RichText::new("Space").strong().small());
            let active_space_id = self.session.document().active_space();
            let active_cat = self
                .session
                .document()
                .active_model_space()
                .map(|s| s.category)
                .unwrap_or(SpaceCategory::Walls);
            let active_label = self
                .session
                .document()
                .active_model_space()
                .map(|s| {
                    if s.category == SpaceCategory::Custom {
                        s.name
                            .rsplit_once('·')
                            .map(|(_, r)| r.trim().to_string())
                            .unwrap_or_else(|| s.name.clone())
                    } else {
                        s.category.as_str().to_string()
                    }
                })
                .unwrap_or_else(|| "Walls".into());
            let spaces_on_floor: Vec<(aadt_model::ModelSpaceId, String, SpaceCategory)> = active_floor
                .map(|fid| {
                    self.session
                        .document()
                        .spaces_on_floor(fid)
                        .into_iter()
                        .map(|s| (s.id, s.name.clone(), s.category))
                        .collect()
                })
                .unwrap_or_default();
            egui::ComboBox::from_id_salt("cat_combo")
                .selected_text(active_label)
                .width(130.0)
                .show_ui(ui, |ui| {
                    for cat in SpaceCategory::ALL {
                        if ui
                            .selectable_label(active_cat == cat, cat.as_str())
                            .clicked()
                        {
                            let fb = self.app_feed(&format!("CATEGORY {}", cat.as_str()));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                            self.on_floor_or_space_changed();
                        }
                    }
                    let customs: Vec<_> = spaces_on_floor
                        .iter()
                        .filter(|(_, _, c)| *c == SpaceCategory::Custom)
                        .collect();
                    if !customs.is_empty() {
                        ui.separator();
                        for (id, name, _) in customs {
                            let short = name
                                .rsplit_once('·')
                                .map(|(_, r)| r.trim())
                                .unwrap_or(name.as_str());
                            if ui
                                .selectable_label(*id == active_space_id, short)
                                .clicked()
                            {
                                let _ = self
                                    .session
                                    .document_mut()
                                    .transact("CATEGORY", |d| d.set_active_space(*id));
                                self.on_floor_or_space_changed();
                                self.note = format!("active model space → {name}");
                                self.dirty = true;
                            }
                        }
                    }
                });
            if ui
                .small_button("+")
                .on_hover_text("Add custom model space on this floor (SPACEADD)")
                .clicked()
            {
                let n = spaces_on_floor
                    .iter()
                    .filter(|(_, _, c)| *c == SpaceCategory::Custom)
                    .count()
                    + 1;
                let fb = self.app_feed(&format!("SPACEADD Space {n}"));
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
                self.on_floor_or_space_changed();
            }
            ui.separator();
            let mut below = self.session.document().trace_below();
            let mut above = self.session.document().trace_above();
            if ui.checkbox(&mut below, "Trace below").changed()
                || ui.checkbox(&mut above, "Trace above").changed()
            {
                let mode = match (below, above) {
                    (true, true) => "BOTH",
                    (true, false) => "BELOW",
                    (false, true) => "ABOVE",
                    (false, false) => "OFF",
                };
                let fb = self.app_feed(&format!("TRACE {mode}"));
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
            }
            ui.separator();
            let shim = self.session.document().active_category_layer();
            let space_name = self
                .session
                .document()
                .active_model_space()
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "—".into());
            ui.label(
                egui::RichText::new(format!("{space_name} → shim {shim}"))
                    .small()
                    .color(color::TEXT_SECONDARY),
            )
            .on_hover_text(
                "New geometry targets the active Floor + Space. The layer name is a DXF/DWG compat shim (ADR-0009), not a separate drafting current layer.",
            );
            if !self.selection.is_empty()
                && ui
                    .small_button("To space")
                    .on_hover_text("Move selection into the active model space (CHSPACE)")
                    .clicked()
            {
                self.sync_selection();
                let fb = self.app_feed("CHSPACE");
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
            }
        });
    }

    pub(super) fn ui_layer_manager(&mut self, ui: &mut egui::Ui) {
        let panel_w = lock_panel_width(ui);
        let narrow = panel_w < 300.0;
        section_label_first(ui, "Scope");
        section_hint(
            ui,
            "DXF/DWG visibility & import mapping. Draft with Floor + Space — new draws use the active category shim.",
        );
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(panel_w);
            ui.spacing_mut().item_spacing.x = space::SM;
            ui.label(
                egui::RichText::new("Show")
                    .size(type_scale::CAPTION)
                    .strong()
                    .color(color::TEXT_SECONDARY),
            );
            egui::ComboBox::from_id_salt("layer_scope")
                .selected_text(self.layer_mgr_scope.label())
                .width((panel_w * 0.38).clamp(80.0, 130.0))
                .show_ui(ui, |ui| {
                    for scope in [
                        LayerMgrScope::ActiveSpace,
                        LayerMgrScope::ActiveFloor,
                        LayerMgrScope::Import,
                        LayerMgrScope::All,
                    ] {
                        if ui
                            .selectable_label(self.layer_mgr_scope == scope, scope.label())
                            .clicked()
                        {
                            self.layer_mgr_scope = scope;
                            self.layer_rows_cache = None;
                        }
                    }
                });
            if ui
                .button("Map layers…")
                .on_hover_text("Map CAD layers to Walls / Structural / Lintel / … spaces")
                .clicked()
            {
                self.open_layer_map_dialog();
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.layer_filter)
                    .desired_width(ui.available_width())
                    .hint_text("Search for layer"),
            );
        });
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(panel_w);
            ui.spacing_mut().item_spacing.x = space::SM;
            if icon_btn(ui, Icon::Add, "New layer").clicked() {
                let name = format!("LAYER{}", self.session.document().layers().count() + 1);
                self.app_feed(&format!("LAYER {name}"));
                self.current_layer = name;
            }
            if icon_btn(ui, Icon::Delete, "Delete unused (PURGE)").clicked() {
                self.app_feed("PURGE LAYERS");
            }
            if icon_btn(
                ui,
                Icon::Select,
                "Highlight shim layer (compat only — drafting uses Floor + Space)",
            )
            .clicked()
            {
                self.note = format!(
                    "Shim highlight → {} (drafting still uses active Floor + Space)",
                    self.session.document().active_category_layer()
                );
            }
            egui::ComboBox::from_id_salt("lset")
                .selected_text(&self.layer_set)
                .width((panel_w * 0.42).clamp(90.0, 150.0))
                .show_ui(ui, |ui| {
                    for name in LayerSet::names() {
                        if ui.selectable_label(self.layer_set == name, name).clicked() {
                            self.layer_set = name.to_string();
                            let fb = self.app_feed(&format!("LAYERSET {name}"));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                            if name == "IMPORT" {
                                self.layer_mgr_scope = LayerMgrScope::Import;
                                self.layer_rows_cache = None;
                            }
                        }
                    }
                });
        });
        ui.separator();

        // Column headers
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new("On")
                    .small()
                    .strong()
                    .color(color::TEXT_SECONDARY),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Frz")
                    .small()
                    .strong()
                    .color(color::TEXT_SECONDARY),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Lck")
                    .small()
                    .strong()
                    .color(color::TEXT_SECONDARY),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Clr")
                    .small()
                    .strong()
                    .color(color::TEXT_SECONDARY),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Name")
                    .small()
                    .strong()
                    .color(color::TEXT_SECONDARY),
            );
            if !narrow {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("Space")
                            .small()
                            .strong()
                            .color(color::TEXT_SECONDARY),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("LW")
                            .small()
                            .strong()
                            .color(color::TEXT_SECONDARY),
                    );
                });
            }
        });
        ui.separator();

        let rows = self.layer_rows_snapshot();
        let total = rows.len();
        let scope_label = self.layer_mgr_scope.label();
        let space_hint = self
            .session
            .document()
            .active_model_space()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "—".into());
        let shim = self.session.document().active_category_layer();
        let map_snapshot: std::collections::BTreeMap<String, aadt_model::SpaceCategory> =
            self.session.document().layer_space_map().clone();
        let combo_w = if narrow {
            (panel_w - space::MD).clamp(100.0, 180.0)
        } else {
            88.0
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            let _ = lock_panel_width(ui);
            for (name, color_aci, visible, frozen, locked, lw, _lt) in rows {
                let is_current = self.current_layer == name || name == shim;
                let mapped = map_snapshot
                    .get(&name)
                    .copied()
                    .unwrap_or_else(|| aadt_model::SpaceCategory::from_layer_name(&name));
                ui.horizontal_wrapped(|ui| {
                    ui.set_max_width(panel_w);
                    ui.spacing_mut().item_spacing.x = space::XS;
                    if is_current {
                        ui.colored_label(color::ACCENT, "●");
                    } else {
                        ui.label(egui::RichText::new("·").color(color::STROKE));
                    }
                    let eye = if visible { Icon::Eye } else { Icon::EyeOff };
                    if icon_btn_compact(ui, eye, "On / Off", !visible).clicked() {
                        let cmd = if visible { "OFF" } else { "ON" };
                        self.app_feed(&format!("LAYER {name} {cmd}"));
                    }
                    if icon_btn_compact(ui, Icon::Freeze, "Freeze / Thaw", frozen).clicked() {
                        let cmd = if frozen { "THAW" } else { "FREEZE" };
                        self.app_feed(&format!("LAYER {name} {cmd}"));
                    }
                    let lock_icon = if locked { Icon::Lock } else { Icon::Unlock };
                    if icon_btn_compact(ui, lock_icon, "Lock / Unlock", locked).clicked() {
                        let cmd = if locked { "UNLOCK" } else { "LOCK" };
                        self.app_feed(&format!("LAYER {name} {cmd}"));
                    }
                    let (r, g, b) = aci_to_srgb(color_aci);
                    let (sw, _) = ui.allocate_painter(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        sw.rect,
                        radius::SMALL,
                        egui::Color32::from_rgb(r, g, b),
                    );
                    ui.painter().rect_stroke(
                        sw.rect,
                        radius::SMALL,
                        egui::Stroke::new(stroke::THIN, color::STROKE),
                    );
                    let display = name
                        .strip_prefix(aadt_model::IMPORTED_LAYER_PREFIX)
                        .unwrap_or(&name);
                    let name_max = if narrow { 18 } else { 28 };
                    let name_resp = ui
                        .selectable_label(is_current, truncate_ui(display, name_max))
                        .on_hover_text(format!(
                            "{name}\nShim highlight only — new geometry follows Floor + Space (ADR-0009)."
                        ));
                    if name_resp.clicked() {
                        self.current_layer = name.clone();
                        self.note = format!(
                            "Shim highlight → {name} (drafting still uses Floor + Space)"
                        );
                    }
                    if name_resp.double_clicked() {
                        self.current_layer = name.clone();
                        self.note = format!("Layer {name} maps to space {mapped}");
                    }
                    if !narrow {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let mut cat = mapped;
                            egui::ComboBox::from_id_salt(format!("lmap_row_{name}"))
                                .selected_text(cat.as_str())
                                .width(combo_w)
                                .show_ui(ui, |ui| {
                                    for c in aadt_model::SpaceCategory::ALL {
                                        if ui.selectable_label(cat == c, c.as_str()).clicked() {
                                            cat = c;
                                        }
                                    }
                                });
                            if cat != mapped {
                                let fb = self.app_feed(&format!("LAYERMAP {name} {}", cat.as_str()));
                                if let Some(m) = fb.message() {
                                    self.note = m.to_string();
                                }
                                let floor = self.session.document().active_floor_id();
                                if let Some(floor) = floor {
                                    self.session.document_mut().reassign_by_layer_map(floor);
                                    self.dirty = true;
                                    self.layer_rows_cache = None;
                                }
                            }
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
                    }
                });
                if narrow {
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(panel_w);
                        ui.add_space(18.0);
                        ui.small(format!("LW {lw:.2}"));
                        let mut cat = mapped;
                        egui::ComboBox::from_id_salt(format!("lmap_row_{name}"))
                            .selected_text(cat.as_str())
                            .width(combo_w)
                            .show_ui(ui, |ui| {
                                for c in aadt_model::SpaceCategory::ALL {
                                    if ui.selectable_label(cat == c, c.as_str()).clicked() {
                                        cat = c;
                                    }
                                }
                            });
                        if cat != mapped {
                            let fb = self.app_feed(&format!("LAYERMAP {name} {}", cat.as_str()));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                            let floor = self.session.document().active_floor_id();
                            if let Some(floor) = floor {
                                self.session.document_mut().reassign_by_layer_map(floor);
                                self.dirty = true;
                                self.layer_rows_cache = None;
                            }
                        }
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
                    });
                }
            }
        });
        ui.separator();
        ui.small(format!(
            "{scope_label}: {total} layers · active {space_hint}"
        ));
    }

    pub(super) fn ui_blocks_manager(&mut self, ui: &mut egui::Ui) {
        if std::env::var("AADT_DEBUG_BLOCKS_STUB").is_ok() {
            ui.label("blocks stub");
            return;
        }
        self.ensure_blocks_cache();
        let cat = {
            let doc = self.session.document();
            doc.model_space(doc.active_space())
                .map(|s| s.category)
                .unwrap_or(aadt_model::SpaceCategory::Walls)
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let panel_w = lock_panel_width(ui);
                let tile = fit_tile_size(panel_w, 52.0, 96.0, space::SM);
                let half = half_button_width(panel_w);

                ui.label(
                    egui::RichText::new(format!("Active space: {}", cat.as_str()))
                        .small()
                        .color(color::ACCENT),
                );
                ui.add_space(space::XS);

                ui.label(egui::RichText::new("Create").strong());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = space::SM;
                    if ui
                        .add_sized([half, 24.0], egui::Button::new("BLOCK"))
                        .on_hover_text("Create from selection")
                        .clicked()
                    {
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
                    if ui
                        .add_sized([half, 24.0], egui::Button::new("WBLOCK"))
                        .on_hover_text("Create & erase sources")
                        .clicked()
                    {
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
                });

                ui.add_space(space::SM);
                ui.label(egui::RichText::new("Place").strong());
                let place = place_kinds_for_category(cat);
                if place.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "No place tools for this space — switch category or load library.",
                        )
                        .small()
                        .color(color::TEXT_SECONDARY),
                    );
                } else {
                    // Seed so place-block previews have geometry.
                    aadt_command::ensure_place_blocks(self.session.document_mut());
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                        ui.set_max_width(panel_w);
                        for (label, kind) in place {
                            let bname = kind.block_name();
                            let selected = self.insert_block.eq_ignore_ascii_case(bname)
                                && matches!(self.tool, Tool::PlaceElem { kind: k } if k == *kind);
                            let resp = block_preview_tile(
                                ui,
                                self.session.document(),
                                bname,
                                *label,
                                selected,
                                tile,
                            );
                            if resp.clicked() {
                                aadt_command::ensure_place_blocks(self.session.document_mut());
                                self.tool = Tool::PlaceElem { kind: *kind };
                                self.insert_block = bname.to_string();
                                self.note = format!("{bname} — click to place");
                            }
                        }
                    });
                }

                ui.add_space(space::SM);
                ui.separator();
                let blocks: Vec<(String, usize)> = self
                    .blocks_cache
                    .iter()
                    .filter(|(name, _)| block_name_for_category(name, cat))
                    .cloned()
                    .collect();
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("In drawing").strong());
                    ui.label(
                        egui::RichText::new(format!("({})", blocks.len()))
                            .small()
                            .color(color::TEXT_SECONDARY),
                    );
                });
                if blocks.is_empty() {
                    ui.label(
                        egui::RichText::new("No blocks for this space — Place or Load library.")
                            .small()
                            .color(color::TEXT_SECONDARY),
                    );
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                        ui.set_max_width(panel_w);
                        for (name, n) in &blocks {
                            let selected = self.insert_block.eq_ignore_ascii_case(name)
                                && matches!(self.tool, Tool::Insert { .. });
                            let caption = format!("{name} ({n})");
                            let resp = block_preview_tile(
                                ui,
                                self.session.document(),
                                name,
                                &caption,
                                selected,
                                tile,
                            );
                            if resp.on_hover_text(format!("INSERT {name}")).clicked() {
                                self.insert_block = name.clone();
                                self.tool = Tool::Insert { name: name.clone() };
                                self.note =
                                    format!("INSERT {name} — click to place (Esc finishes)");
                            }
                        }
                    });
                }

                // Selected INSERT editor (compact).
                if let Some(id) = self.selection.first().copied() {
                    let insert_info =
                        self.session
                            .document()
                            .get(id)
                            .and_then(|e| match &e.geometry {
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
                            });
                    if let Some((ins_name, attrs, states)) = insert_info {
                        ui.add_space(space::SM);
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!("Selected {ins_name} (#{})", id.0))
                                .strong()
                                .small(),
                        );
                        for (k, v) in &attrs {
                            ui.horizontal_wrapped(|ui| {
                                let label_w = (panel_w * 0.28).clamp(56.0, 88.0);
                                ui.add_sized(
                                    [label_w, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new(truncate_ui(k, 10)).small(),
                                    ),
                                );
                                let mut edit = v.clone();
                                let edit_w = (ui.available_width() - space::SM).max(60.0);
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut edit)
                                            .desired_width(edit_w)
                                            .clip_text(true),
                                    )
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
                            ui.horizontal_wrapped(|ui| {
                                ui.set_max_width(panel_w);
                                ui.small("Vis");
                                for st in &states {
                                    if ui.small_button(st).clicked() {
                                        let fb =
                                            self.session.feed(&format!("BSETVIS {} {}", id.0, st));
                                        if let Some(m) = fb.message() {
                                            self.note = m.to_string();
                                        }
                                    }
                                }
                            });
                        }
                    }
                }

                ui.add_space(space::SM);
                ui.separator();
                ui.label(egui::RichText::new("Library").strong());
                ui.label(
                    egui::RichText::new(format!(
                        "Filtered to {} — Load scales inches → metres.",
                        cat.as_str()
                    ))
                    .small()
                    .color(color::TEXT_SECONDARY),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = space::SM;
                    if ui
                        .add_sized([half, 24.0], egui::Button::new("Load matching"))
                        .on_hover_text("Import library blocks for this space")
                        .clicked()
                    {
                        self.ensure_library_cache(true);
                        let mut n = 0usize;
                        let entries: Vec<_> = self
                            .library_cache
                            .iter()
                            .filter(|(_, path)| library_path_for_category(path, cat))
                            .cloned()
                            .collect();
                        for (name, path) in entries {
                            let p = path.display().to_string();
                            let _ = self.session.feed(&format!("BLOCKLIB import {p} {name}"));
                            n += 1;
                        }
                        self.note = format!("Imported {n} {} library blocks", cat.as_str());
                        self.blocks_cache.clear();
                        self.blocks_cache_rev = None;
                        self.invalidate_library_cache();
                        aadt_command::ensure_place_blocks(self.session.document_mut());
                    }
                    if ui
                        .add_sized([half, 24.0], egui::Button::new("Seed place"))
                        .clicked()
                    {
                        aadt_command::ensure_place_blocks(self.session.document_mut());
                        self.blocks_cache.clear();
                        self.blocks_cache_rev = None;
                        self.note = "Seeded place blocks".into();
                    }
                });

                self.ensure_library_cache(false);
                let lib: Vec<_> = self
                    .library_cache
                    .iter()
                    .filter(|(_, path)| library_path_for_category(path, cat))
                    .cloned()
                    .collect();
                if lib.is_empty() {
                    ui.small("(no library files for this space)");
                } else {
                    ui.label(
                        egui::RichText::new(format!("{} files", lib.len()))
                            .small()
                            .color(color::TEXT_SECONDARY),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                        ui.set_max_width(panel_w);
                        for (name, path) in &lib {
                            let selected = self.insert_block.eq_ignore_ascii_case(name);
                            // Prefer document geometry if already imported; else folder glyph.
                            let resp = if self.session.document().block(name).is_some() {
                                block_preview_tile(
                                    ui,
                                    self.session.document(),
                                    name,
                                    name,
                                    selected,
                                    tile,
                                )
                            } else {
                                library_preview_tile(ui, name, path, selected, tile)
                            };
                            if resp.on_hover_text(path.display().to_string()).clicked() {
                                let p = path.display().to_string();
                                let fb = self.session.feed(&format!("BLOCKLIB import {p} {name}"));
                                if let Some(m) = fb.message() {
                                    self.note = m.to_string();
                                }
                                self.insert_block = name.clone();
                                self.tool = Tool::Insert { name: name.clone() };
                                self.blocks_cache.clear();
                                self.blocks_cache_rev = None;
                            }
                        }
                    });
                }

                if !self.insert_block.is_empty() {
                    ui.add_space(space::XS);
                    ui.label(
                        egui::RichText::new(format!("Active: {}", self.insert_block))
                            .small()
                            .color(color::ACCENT),
                    );
                }
            });
    }

    pub(super) fn ui_selection_properties(&mut self, ui: &mut egui::Ui) {
        use aadt_model::Geometry;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let _ = lock_panel_width(ui);
            if self.selection.is_empty() {
                ui.label(
                    egui::RichText::new("No selection")
                        .strong()
                        .color(color::TEXT_SECONDARY),
                );
                ui.label(
                    egui::RichText::new(
                        "Click objects in the active Floor + Space to inspect layer, space, and geometry.",
                    )
                    .small()
                    .weak(),
                );
                return;
            }
            ui.label(
                egui::RichText::new(format!("{} object(s)", self.selection.len()))
                    .strong(),
            );
            ui.add_space(space::SM);
            let ids: Vec<_> = self.selection.clone();
            for id in ids {
                let Some(e) = self.session.document().get(id) else {
                    continue;
                };
                let kind = match &e.geometry {
                    Geometry::Line(_) => "LINE",
                    Geometry::Polyline(p) if p.closed => "RECT/PLINE (closed)",
                    Geometry::Polyline(_) => "POLYLINE",
                    Geometry::Circle(_) => "CIRCLE",
                    Geometry::Arc(_) => "ARC",
                    Geometry::Text(_) => "TEXT",
                    Geometry::Insert(i) => {
                        ui.group(|ui| {
                            ui.label(egui::RichText::new(format!("#{} INSERT", id.0)).strong());
                            ui.label(format!("block  {}", i.name));
                            ui.label(format!("layer  {}", e.layer));
                            let space = self
                                .session
                                .document()
                                .model_space(e.space)
                                .map(|s| s.name.clone())
                                .unwrap_or_else(|| format!("space {}", e.space.0));
                            ui.label(format!("space  {space}"));
                        });
                        continue;
                    }
                };
                ui.group(|ui| {
                    ui.label(egui::RichText::new(format!("#{} {kind}", id.0)).strong());
                    ui.label(format!("layer  {}", e.layer));
                    let space = self
                        .session
                        .document()
                        .model_space(e.space)
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| format!("space {}", e.space.0));
                    ui.label(format!("space  {space}"));
                });
                ui.add_space(space::XS);
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Move to active space")
                    .on_hover_text("CHSPACE — move selection into the active model space")
                    .clicked()
                {
                    self.sync_selection();
                    let fb = self.app_feed("CHSPACE");
                    if let Some(m) = fb.message() {
                        self.note = m.to_string();
                    }
                }
                if ui.button("Erase").clicked() {
                    self.erase_selection();
                }
            });
        });
    }

    pub(super) fn ui_history_panel(&mut self, ui: &mut egui::Ui) {
        let _ = lock_panel_width(ui);
        section_label_first(ui, "Versions");
        section_hint(
            ui,
            "File checkpoints inside .aadt — not the same as UNDO.",
        );
        let _ = full_width_edit(ui, &mut self.history_label, "Issued for construction");
        ui.add_space(panel_space::TIGHT);
        if full_width_button(ui, "Save version").clicked() {
            self.history_label = if self.history_label.trim().is_empty() {
                "Named version".into()
            } else {
                self.history_label.clone()
            };
            self.app_feed(&format!(
                "HISTORY SAVE {}",
                self.history_label.replace(' ', "_")
            ));
            if let Some(fb) = self.session.transcript().last() {
                if let Some(msg) = fb.message() {
                    self.note = msg.to_string();
                }
            }
        }
        ui.add_space(panel_space::TIGHT);
        let (refresh, purge) = two_col_buttons(ui, "Refresh list", "Purge autos");
        if refresh.clicked() {
            self.app_feed("HISTORY LIST");
            if let Some(fb) = self.session.transcript().last() {
                if let Some(msg) = fb.message() {
                    self.note = msg.to_string();
                }
            }
        }
        if purge.clicked() {
            self.app_feed("HISTORY PURGE");
        }

        section_label(ui, "Checkpoints");
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
                            section_hint(ui, "No checkpoints yet — SAVE the drawing.");
                        }
                        for c in list.iter().take(40) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "#{} [{}] {}",
                                        c.id, c.kind, c.label
                                    ))
                                    .size(type_scale::CAPTION),
                                );
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
                                    "{} · rev {}",
                                    c.change_summary, c.doc_revision
                                ))
                                .size(type_scale::CAPTION)
                                .color(color::TEXT_SECONDARY),
                            );
                            ui.add_space(panel_space::TIGHT);
                        }
                    }
                    Err(e) => {
                        ui.colored_label(color::DANGER, format!("History: {e}"));
                    }
                }
            } else {
                section_hint(ui, "Save as .aadt to enable the time machine.");
            }
        } else {
            section_hint(ui, "OPEN or SAVE a .aadt drawing to use History.");
        }
    }

    /// Top quick-access bar — File / Edit / Plot.
    pub(super) fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("qat")
            .exact_height(32.0)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_BASE)
                    .inner_margin(egui::Margin::symmetric(space::SM, space::XS))
                    .stroke(egui::Stroke::new(stroke::THIN, color::BORDER)),
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
                            .add_filter("AADT / DXF / DWG", &["aadt", "dxf", "dwg"])
                            .add_filter("DWG", &["dwg"])
                            .add_filter("DXF", &["dxf"])
                            .add_filter("AADT", &["aadt"])
                            .pick_file()
                        {
                            let p = path.display().to_string();
                            let fb = self.app_feed(&format!("OPEN {p}"));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
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
                    if icon_btn(ui, Icon::Plot, "Plot to PDF… (Ctrl+P)").clicked() {
                        self.seed_plot_dialog();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("AADT").strong().color(color::TEXT));
                        ui.add_space(space::XS);
                        brand_mark_widget(ui, 16.0);
                    });
                });
            });
        // Second strip: Floor / Space / TRACE (primary drafting context).
        egui::TopBottomPanel::top("qat_levels")
            .exact_height(32.0)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_CARD)
                    .inner_margin(egui::Margin::symmetric(space::SM, space::XS))
                    .stroke(egui::Stroke::new(stroke::THIN, color::BORDER)),
            )
            .show(ctx, |ui| {
                self.ui_levels_strip(ui);
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
                    .stroke(egui::Stroke::new(stroke::THIN, color::BORDER)),
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
        let show_hint = self.tool_prompt().is_some() || !self.note.is_empty();
        let token = self.command.trim().to_string();
        let completions = if !token.is_empty() && !token.contains(' ') {
            self.session.complete(&token)
        } else {
            Vec::new()
        };
        let show_comp = !completions.is_empty();
        let bar_h = if show_hint || show_comp { 48.0 } else { 28.0 };

        egui::TopBottomPanel::bottom("status")
            .exact_height(bar_h)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_BASE)
                    .inner_margin(egui::Margin::symmetric(space::SM, space::XS))
                    .stroke(egui::Stroke::new(stroke::THIN, color::BORDER)),
            )
            .show(ctx, |ui| {
                // Row 1: command + status (status reserved on the right so text never collides).
                ui.horizontal(|ui| {
                    let units =
                        aadt_model::primary_from_props(&self.session.document().properties().units);
                    let units_chip = ui.add(
                        egui::Button::new(
                            egui::RichText::new(units.as_token())
                                .strong()
                                .color(color::ACCENT),
                        )
                        .fill(color::ACCENT_SOFT)
                        .stroke(egui::Stroke::new(stroke::THIN, color::ACCENT))
                        .rounding(radius::CONTROL),
                    );
                    if units_chip
                        .on_hover_text(format!("{} — open Units manager", units.label()))
                        .clicked()
                    {
                        self.show_layer_mgr = true;
                        self.right_tab = 4;
                        self.props_edit = self.session.document().properties().clone();
                        self.persist_prefs();
                    }
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

                    const STATUS_RESERVE: f32 = 500.0;
                    let cmd_w = (ui.available_width() - STATUS_RESERVE).max(120.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.command)
                            .desired_width(cmd_w)
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

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let n = self.session.document().len();
                        ui.label(
                            egui::RichText::new(format!("{n} ents"))
                                .small()
                                .color(color::TEXT_SECONDARY),
                        );
                        if self.dirty {
                            ui.colored_label(color::WARNING, "MOD");
                        }
                        if let Some(p) = &self.file_path {
                            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("drawing");
                            let short = if name.len() > 14 {
                                format!("…{}", &name[name.len() - 12..])
                            } else {
                                name.to_string()
                            };
                            ui.label(
                                egui::RichText::new(short)
                                    .small()
                                    .color(color::TEXT_SECONDARY),
                            );
                        }
                        // RTL: add aids rightmost-first so they read ORTHO…LWT left-to-right.
                        let mut aids_changed = false;
                        for (on, label, tip) in [
                            (
                                self.show_lweights,
                                "LWT",
                                "Lineweights display (click to toggle)",
                            ),
                            (self.dyn_mode, "DYN", "Dynamic Input F12 (click to toggle)"),
                            (
                                self.osnap_track,
                                "OTRACK",
                                "Object-snap tracking F11 (click to toggle)",
                            ),
                            (
                                self.polar,
                                "POLAR",
                                "Polar tracking F10 — 45° (click to toggle)",
                            ),
                            (self.osnap, "OSNAP", "Object snap F3 (click to toggle)"),
                            (self.ortho, "ORTHO", "Ortho F8 (click to toggle)"),
                        ] {
                            let text = if on {
                                egui::RichText::new(label)
                                    .small()
                                    .strong()
                                    .color(color::TEXT)
                            } else {
                                egui::RichText::new(label)
                                    .small()
                                    .color(color::TEXT_SECONDARY)
                            };
                            let resp = ui
                                .add(
                                    egui::Button::new(text)
                                        .fill(if on {
                                            color::ACCENT_SOFT
                                        } else {
                                            color::CONTROL
                                        })
                                        .stroke(egui::Stroke::new(
                                            1.0_f32,
                                            if on { color::ACCENT } else { color::STROKE },
                                        ))
                                        .rounding(radius::CONTROL),
                                )
                                .on_hover_text(tip);
                            if resp.clicked() {
                                aids_changed = true;
                                match label {
                                    "ORTHO" => self.ortho = !self.ortho,
                                    "OSNAP" => {
                                        self.osnap = !self.osnap;
                                        if !self.osnap {
                                            self.snap_hit = None;
                                        }
                                    }
                                    "POLAR" => self.polar = !self.polar,
                                    "OTRACK" => {
                                        self.osnap_track = !self.osnap_track;
                                        if self.osnap_track {
                                            self.osnap = true;
                                            self.polar = true;
                                        }
                                    }
                                    "DYN" => self.dyn_mode = !self.dyn_mode,
                                    "LWT" => self.show_lweights = !self.show_lweights,
                                    _ => {}
                                }
                            }
                        }
                        if aids_changed {
                            self.persist_prefs();
                        }
                        if let Some(c) = self.cursor {
                            ui.monospace(format!("{:.2},{:.2}", c.x, c.y));
                        }
                        egui::ComboBox::from_id_salt("opts_menu")
                            .selected_text("Options")
                            .width(72.0)
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
                                if ui
                                    .checkbox(&mut self.osnap_track, "Osnap track (F11)")
                                    .changed()
                                {
                                    if self.osnap_track {
                                        self.osnap = true;
                                        self.polar = true;
                                    }
                                    self.persist_prefs();
                                }
                                if ui
                                    .checkbox(&mut self.show_lweights, "Lineweights")
                                    .changed()
                                {
                                    self.persist_prefs();
                                }
                                if ui.button("Keyboard help (F1)").clicked() {
                                    self.show_shortcuts = true;
                                }
                            });
                    });
                });

                // Row 2: completions + prompt/note (never shares the status rail).
                if show_comp || show_hint {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = space::XS;
                        for m in completions.iter().take(5) {
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
                        if let Some(prompt) = self.tool_prompt() {
                            let short = truncate_ui(prompt, 72);
                            ui.label(
                                egui::RichText::new(short)
                                    .small()
                                    .strong()
                                    .color(color::ACCENT),
                            );
                        } else if !self.note.is_empty() {
                            let short = truncate_ui(&self.note, 72);
                            ui.label(
                                egui::RichText::new(short)
                                    .small()
                                    .color(egui::Color32::from_rgb(140, 100, 20)),
                            )
                            .on_hover_text(&self.note);
                        }
                    });
                }
            });
    }

    pub(super) fn draw_left_panel(&mut self, ctx: &egui::Context) {
        if !self.show_left_mgr {
            egui::SidePanel::left("left_mgr_collapsed")
                .exact_width(panel_space::RAIL)
                .resizable(false)
                .frame(
                    egui::Frame::none()
                        .fill(color::LAYER_BASE)
                        .stroke(egui::Stroke::new(stroke::THIN, color::BORDER)),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(space::SM);
                        if icon_btn_nav(ui, Icon::ExpandPanel, "Expand structure", false).clicked()
                        {
                            self.show_left_mgr = true;
                            self.persist_prefs();
                        }
                        ui.add_space(space::MD);
                        for dest in theme::left_manager_nav() {
                            let tip = format!("{} — {}", dest.label, dest.tip);
                            if icon_btn_nav(ui, dest.icon, &tip, self.left_tab == dest.tab)
                                .clicked()
                            {
                                self.show_left_mgr = true;
                                self.left_tab = dest.tab;
                                self.persist_prefs();
                            }
                        }
                    });
                });
            return;
        }
        egui::SidePanel::left("left_mgr")
            .default_width(self.left_panel_width)
            .min_width(180.0)
            .max_width(260.0)
            .resizable(true)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_CARD)
                    .inner_margin(egui::Margin::symmetric(panel_space::GUTTER, space::MD))
                    .stroke(egui::Stroke::new(stroke::THIN, color::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    panel_title(ui, left_manager_title(self.left_tab));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_btn(ui, Icon::CollapsePanel, "Collapse to rail").clicked() {
                            self.show_left_mgr = false;
                            self.persist_prefs();
                        }
                    });
                });
                {
                    let doc = self.session.document();
                    let cat = doc
                        .model_space(doc.active_space())
                        .map(|s| s.category)
                        .unwrap_or(aadt_model::SpaceCategory::Walls);
                    let floor = doc
                        .active_floor_id()
                        .and_then(|fid| doc.floor(fid).map(|f| f.name.clone()))
                        .unwrap_or_else(|| "—".into());
                    panel_subtitle(ui, format!("{floor} · {}", cat.as_str()));
                }
                ui.add_space(panel_space::TIGHT);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = space::SM;
                    for dest in theme::left_manager_nav() {
                        let tip = format!("{} — {}", dest.label, dest.tip);
                        if icon_btn_nav(ui, dest.icon, &tip, self.left_tab == dest.tab).clicked()
                        {
                            self.left_tab = dest.tab;
                            self.persist_prefs();
                        }
                        ui.label(
                            egui::RichText::new(dest.label)
                                .size(type_scale::CAPTION)
                                .strong()
                                .color(if self.left_tab == dest.tab {
                                    color::ACCENT
                                } else {
                                    color::TEXT_SECONDARY
                                }),
                        );
                    }
                });
                ui.add_space(panel_space::ROW);
                ui.separator();
                ui.add_space(panel_space::ROW);
                match self.left_tab {
                    1 => self.ui_blocks_manager(ui),
                    _ => self.ui_layer_manager(ui),
                }
                let w = ui.max_rect().width().clamp(200.0, 480.0);
                if (w - self.left_panel_width).abs() > 4.0 {
                    self.left_panel_width = w;
                }
            });
    }

    pub(super) fn draw_right_panel(&mut self, ctx: &egui::Context) {
        if !self.show_layer_mgr {
            egui::SidePanel::right("layer_mgr_collapsed")
                .exact_width(panel_space::RAIL)
                .resizable(false)
                .frame(
                    egui::Frame::none()
                        .fill(color::LAYER_BASE)
                        .stroke(egui::Stroke::new(stroke::THIN, color::BORDER)),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(space::SM);
                        if icon_btn_nav(ui, Icon::ExpandPanel, "Expand tools", false).clicked() {
                            self.show_layer_mgr = true;
                            self.persist_prefs();
                        }
                        ui.add_space(space::MD);
                        for dest in theme::right_manager_nav() {
                            let tip = format!("{} — {}", dest.label, dest.tip);
                            if icon_btn_nav(ui, dest.icon, &tip, self.right_tab == dest.tab)
                                .clicked()
                            {
                                self.show_layer_mgr = true;
                                self.right_tab = dest.tab;
                                if matches!(dest.tab, 4 | 5) {
                                    self.props_edit = self.session.document().properties().clone();
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
            .min_width(200.0)
            .max_width(340.0)
            .resizable(true)
            .frame(
                egui::Frame::none()
                    .fill(color::LAYER_CARD)
                    .inner_margin(egui::Margin::symmetric(panel_space::GUTTER, space::MD))
                    .stroke(egui::Stroke::new(stroke::THIN, color::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    panel_title(ui, right_manager_title(self.right_tab));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_btn(ui, Icon::CollapsePanel, "Collapse to rail").clicked() {
                            self.show_layer_mgr = false;
                            self.persist_prefs();
                        }
                    });
                });
                panel_subtitle(
                    ui,
                    match self.right_tab {
                        0 => "Quantities, publish, Ask, classify",
                        1 => "Hatch patterns",
                        2 => "Tables, leaders, tags",
                        3 => "Nameplate template layers",
                        4 => "Drawing units",
                        5 => "Dimension styles",
                        6 => "Drawing checkpoints",
                        7 => "Selection properties",
                        _ => "",
                    },
                );
                ui.add_space(panel_space::TIGHT);
                // Compact icon pivot — one row, no wrap soup.
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                    for dest in theme::right_manager_nav() {
                        let tip = format!("{} — {}", dest.label, dest.tip);
                        if icon_btn_nav(ui, dest.icon, &tip, self.right_tab == dest.tab).clicked()
                        {
                            self.right_tab = dest.tab;
                            if matches!(dest.tab, 4 | 5) {
                                self.props_edit = self.session.document().properties().clone();
                            }
                            self.persist_prefs();
                        }
                    }
                });
                ui.add_space(panel_space::ROW);
                ui.separator();
                ui.add_space(panel_space::ROW);
                match self.right_tab {
                    1 => self.ui_hatch_manager(ui),
                    2 => self.ui_annotate_panel(ui),
                    3 => self.ui_templates_panel(ui),
                    4 => self.ui_units_manager(ui),
                    5 => self.ui_dimension_manager(ui),
                    6 => self.ui_history_panel(ui),
                    7 => self.ui_selection_properties(ui),
                    _ => self.ui_hatch_manager(ui),
                }
                let w = ui.max_rect().width().clamp(220.0, 520.0);
                if (w - self.right_panel_width).abs() > 4.0 {
                    self.right_panel_width = w;
                }
            });
    }

    pub(super) fn ui_templates_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let panel_w = lock_panel_width(ui);
            ui.label(egui::RichText::new("Nameplate template").strong());
            ui.label(
                egui::RichText::new(
                    "Draw on TMPL_BORDER / TMPL_VIEW / TMPL_TEXT, make a block, convert it, then INSERT. Text with {title} etc. fills from Drawing Properties.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            if ui
                .add_sized([panel_w, 24.0], egui::Button::new("Ensure template layers"))
                .clicked()
            {
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
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.label("Block:");
                let edit_w = (panel_w * 0.45).clamp(80.0, 160.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.insert_block).desired_width(edit_w),
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
            if !nameplate.is_empty()
                && ui
                    .add_sized(
                        [panel_w, 24.0],
                        egui::Button::new(format!("INSERT {nameplate}")),
                    )
                    .clicked()
            {
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
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
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
        let panel_w = lock_panel_width(ui);
        ui.label(egui::RichText::new("Pattern preview").strong());
        ui.label(
            egui::RichText::new(format!("Active: {}", self.hatch_pattern))
                .small()
                .color(color::ACCENT),
        );
        if ui
            .add_sized([panel_w, 24.0], egui::Button::new("Apply Hatch (H)"))
            .clicked()
        {
            self.tool = Tool::Hatch;
            self.note = format!(
                "HATCH ({}) — click a closed polyline or circle",
                self.hatch_pattern
            );
        }
        // Edit: retarget the pattern of the selected hatch(es) to the active one.
        let sel = self.selected_hatches.len();
        if sel > 0 {
            let label = format!(
                "Set {sel} selected hatch{} to {}",
                if sel == 1 { "" } else { "es" },
                self.hatch_pattern
            );
            if ui.add_sized([panel_w, 24.0], egui::Button::new(label)).clicked() {
                self.apply_pattern_to_selected_hatches();
            }
        }
        ui.separator();
        let root = aadt_library::default_library_root();
        let patterns = aadt_library::list_hatch_patterns(&root)
            .unwrap_or_else(|_| aadt_library::builtin_hatch_patterns());
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let gap = space::SM;
                let avail = lock_panel_width(ui);
                let tile = fit_tile_size(avail, 48.0, 88.0, gap);
                // Explicit rows: `horizontal_wrapped` under-wraps inside a scroll
                // area (tiles overflow the panel edge instead of flowing down), so
                // chunk into a fixed column count derived from the real width.
                let cols = (((avail + gap) / (tile + gap)).floor() as usize).clamp(1, 5);
                for chunk in patterns.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                        for pat in chunk {
                            let selected = self.hatch_pattern.eq_ignore_ascii_case(&pat.name);
                            let tip = format!("{} — {}", pat.name, pat.description);
                            if hatch_preview_tile(ui, &pat.name, selected, tile)
                                .on_hover_text(tip)
                                .clicked()
                            {
                                self.hatch_pattern = pat.name.clone();
                            }
                        }
                    });
                }
            });
    }

    pub(super) fn ui_annotate_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let panel_w = lock_panel_width(ui);
            ui.label(egui::RichText::new("Insert").strong());
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.label("Table rows");
                ui.add(egui::TextEdit::singleline(&mut self.table_rows).desired_width(40.0));
                ui.label("cols");
                ui.add(egui::TextEdit::singleline(&mut self.table_cols).desired_width(40.0));
            });
            if ui
                .add_sized([panel_w, 24.0], egui::Button::new("Insert Table…"))
                .clicked()
            {
                self.tool = Tool::TablePlace;
            }
            ui.separator();
            ui.label(egui::RichText::new("Leaders & marks").strong());
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
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
            });
            ui.separator();
            ui.label(egui::RichText::new("Dimensions").strong());
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
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
                self.right_tab = 5; // Dims
            }
            ui.separator();
            ui.label(egui::RichText::new("Object count").strong());
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
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
            });
            ui.separator();
            ui.label(egui::RichText::new("Auto-number").strong());
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.label("Prefix");
                ui.add(
                    egui::TextEdit::singleline(&mut self.autonum_prefix)
                        .desired_width((panel_w * 0.35).clamp(48.0, 100.0)),
                );
                ui.label("Start");
                ui.add(egui::TextEdit::singleline(&mut self.autonum_start).desired_width(48.0));
            });
            ui.label(
                egui::RichText::new("Select TEXT / INSERT / CIRCLE objects, then Number.")
                    .small()
                    .weak(),
            );
            if ui
                .add_sized([panel_w, 24.0], egui::Button::new("Number selection"))
                .clicked()
            {
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
            ui.label(egui::RichText::new("Building heights (general notes)").strong());
            ui.label(
                egui::RichText::new(
                    "Declared heights are inserted into suggested general notes and can use {floor_height} / {lintel_height} in nameplate text.",
                )
                .small()
                .weak(),
            );
            prop_field(ui, "Floor-to-floor height", &mut self.props_edit.floor_height);
            prop_field(ui, "Lintel height (above FFL)", &mut self.props_edit.lintel_height);
            ui.horizontal(|ui| {
                if ui
                    .button("Insert height notes")
                    .on_hover_text("Append floor/lintel height lines into Standard notes")
                    .clicked()
                {
                    let extra = aadt_docs::height_notes(&self.props_edit);
                    if extra.is_empty() {
                        self.note = "Set floor height and/or lintel height first".into();
                    } else {
                        let mut lines = aadt_docs::notes_from_text(&self.props_edit.standard_notes);
                        for n in extra {
                            if !lines.iter().any(|l| l == &n) {
                                lines.push(n);
                            }
                        }
                        self.props_edit.standard_notes = aadt_docs::notes_to_text(&lines);
                        self.note = "Height notes inserted into standard notes".into();
                    }
                }
                if ui
                    .button("Reload suggested notes")
                    .on_hover_text("Replace standard notes with detection + height suggestions")
                    .clicked()
                {
                    let det = aadt_detect::detect(
                        self.session.document(),
                        &aadt_detect::DetectionConfig::default(),
                    );
                    let floors = self.session.document().floors();
                    self.props_edit.standard_notes = aadt_docs::notes_to_text(
                        &aadt_docs::NotesLibrary::standard().suggest_for_drawing(
                            &det,
                            &self.props_edit,
                            &floors,
                        ),
                    );
                    self.note = "Standard notes reloaded from suggestion".into();
                }
            });
            ui.separator();
            ui.label(egui::RichText::new("Floors & model spaces").strong());
            ui.label(
                egui::RichText::new(
                    "Each floor has Walls / Structural / Lintel / BeamLayout / Furniture. Add custom spaces for other disciplines.",
                )
                .small()
                .weak(),
            );
            {
                let floor_rows: Vec<(String, u32, f64, usize)> = self
                    .session
                    .document()
                    .floors()
                    .into_iter()
                    .map(|f| {
                        let n = self.session.document().spaces_on_floor(f.id).len();
                        (f.name.clone(), f.index, f.elevation_m, n)
                    })
                    .collect();
                for (name, idx, elev, nspaces) in &floor_rows {
                    ui.label(format!(
                        "[{idx}] {name}  elev {elev:.3} m  ·  {nspaces} spaces"
                    ));
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Add floor").clicked() {
                    let n = self.session.document().floors().len() + 1;
                    let fb = self.app_feed(&format!("FLOORADD Level {n}"));
                    if let Some(m) = fb.message() {
                        self.note = m.to_string();
                    }
                    self.sync_layer_from_active_space();
                    self.dirty = true;
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.space_add_draft)
                        .desired_width(140.0)
                        .hint_text("Custom space name"),
                );
                if ui.button("Add space").clicked() {
                    let name = self.space_add_draft.trim().to_string();
                    if name.is_empty() {
                        self.note = "Enter a custom space name".into();
                    } else {
                        let fb = self.app_feed(&format!("SPACEADD {name}"));
                        if let Some(m) = fb.message() {
                            self.note = m.to_string();
                        }
                        self.space_add_draft.clear();
                        self.sync_layer_from_active_space();
                        self.dirty = true;
                    }
                }
            });
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
            let panel_w = lock_panel_width(ui);
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
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(ui.available_width());
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
                self.right_tab = 5; // Dims
            }

            ui.separator();
            let half = half_button_width(panel_w);
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing.x = space::SM;
                if ui.add_sized([half, 24.0], egui::Button::new("Apply")).clicked() {
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
                if ui.add_sized([half, 24.0], egui::Button::new("Reload")).clicked() {
                    self.props_edit = self.session.document().properties().clone();
                }
            });
        });
    }

    pub(super) fn ui_dimension_manager(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let panel_w = lock_panel_width(ui);
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
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
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
                self.props_edit.dim_secondary = if secondary { "1".into() } else { "0".into() };
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
                self.right_tab = 4; // Units
            }

            ui.horizontal(|ui| {
                if ui.button("Apply style").clicked() {
                    let sys = aadt_model::primary_from_props(&self.props_edit.units);
                    if self.props_edit.dimstyle.is_empty() {
                        self.props_edit.dimstyle = sys.dimstyle_name().to_string();
                    }
                    self.session.set_drawing_properties(self.props_edit.clone());
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
                    .stroke(egui::Stroke::new(stroke::THIN, color::BORDER))
                    .rounding(radius::DIALOG),
            )
            .show(ctx, |ui| {
                ui.label("Always exports a vector PDF (no direct print).");
                ui.label(
                    "Choose what to place on the sheet: full extents, selection, or a window.",
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Plot area");
                    if ui
                        .selectable_label(self.plot_area_mode == 0, "Extents")
                        .clicked()
                    {
                        self.plot_area_mode = 0;
                    }
                    if ui
                        .selectable_label(self.plot_area_mode == 1, "Selection")
                        .on_hover_text(
                            "Uses the current selection (falls back to extents if empty)",
                        )
                        .clicked()
                    {
                        self.plot_area_mode = 1;
                    }
                    if ui
                        .selectable_label(self.plot_area_mode == 2, "Window")
                        .on_hover_text("Pick two corners on the canvas")
                        .clicked()
                    {
                        self.plot_area_mode = 2;
                    }
                    if self.plot_area_mode == 2 {
                        if ui.button("Pick window…").clicked() {
                            self.plot_open = false;
                            self.tool = super::Tool::PlotWindow { first: None };
                            self.note = "PLOT WINDOW — click first corner".into();
                        }
                        if let Some(w) = self.plot_window {
                            ui.label(
                                egui::RichText::new(format!(
                                    "({:.2},{:.2})–({:.2},{:.2})",
                                    w.min.x, w.min.y, w.max.x, w.max.y
                                ))
                                .small()
                                .color(color::TEXT_SECONDARY),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("no window yet")
                                    .small()
                                    .color(color::TEXT_SECONDARY),
                            );
                        }
                    }
                });
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
                                if ui.selectable_label(self.plot_paper == name, name).clicked() {
                                    self.plot_paper = name.to_string();
                                }
                            }
                        });
                    let content = match self.plot_area_mode {
                        1 => aadt_docs::PlotContent::Selection(&self.selection),
                        2 => match self.plot_window {
                            Some(w) => aadt_docs::PlotContent::Window(w),
                            None => aadt_docs::PlotContent::Extents,
                        },
                        _ => aadt_docs::PlotContent::Extents,
                    };
                    let orient = match content {
                        aadt_docs::PlotContent::Window(w) => aadt_docs::orientation_from_bbox(w),
                        aadt_docs::PlotContent::Selection(ids) => {
                            aadt_docs::orientation_from_selection(self.session.document(), ids)
                        }
                        aadt_docs::PlotContent::Extents => {
                            aadt_docs::orientation_from_extents(self.session.document())
                        }
                    };
                    let layout = aadt_docs::TitleBlockLayout::for_orientation(orient);
                    let label = match (self.plot_area_mode, orient, layout) {
                        (
                            1,
                            aadt_docs::Orientation::Landscape,
                            aadt_docs::TitleBlockLayout::VerticalRight,
                        ) => "Selection · Landscape · title right",
                        (
                            1,
                            aadt_docs::Orientation::Portrait,
                            aadt_docs::TitleBlockLayout::HorizontalBottom,
                        ) => "Selection · Portrait · title bottom",
                        (
                            2,
                            aadt_docs::Orientation::Landscape,
                            aadt_docs::TitleBlockLayout::VerticalRight,
                        ) => "Window · Landscape · title right",
                        (
                            2,
                            aadt_docs::Orientation::Portrait,
                            aadt_docs::TitleBlockLayout::HorizontalBottom,
                        ) => "Window · Portrait · title bottom",
                        (
                            _,
                            aadt_docs::Orientation::Landscape,
                            aadt_docs::TitleBlockLayout::VerticalRight,
                        ) => "Extents · Landscape · title right",
                        (
                            _,
                            aadt_docs::Orientation::Portrait,
                            aadt_docs::TitleBlockLayout::HorizontalBottom,
                        ) => "Extents · Portrait · title bottom",
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
                        let props = self.session.document().properties().clone();
                        let floors = self.session.document().floors();
                        self.plot_notes = aadt_docs::notes_to_text(
                            &aadt_docs::NotesLibrary::standard()
                                .suggest_for_drawing(&det, &props, &floors),
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
        let content = match self.plot_area_mode {
            1 => aadt_docs::PlotContent::Selection(&self.selection),
            2 => match self.plot_window {
                Some(w) => aadt_docs::PlotContent::Window(w),
                None => {
                    self.note = "Plot window not set — pick a window or use Extents".into();
                    return;
                }
            },
            _ => aadt_docs::PlotContent::Extents,
        };
        let (mut sheet_doc, sheet, composed) =
            aadt_docs::compose_plot_sheet_for_area(self.session.document(), paper, &tb, content);

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

    pub(super) fn draw_layer_map_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_layer_map {
            return;
        }
        let mut open = true;
        let counts = self.session.document().layer_usage_counts();
        egui::Window::new("Map layers → spaces")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(color::LAYER_CARD)
                    .stroke(egui::Stroke::new(stroke::THIN, color::BORDER))
                    .rounding(radius::DIALOG),
            )
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Assign each imported CAD layer to a model-space category. Apply moves geometry into those spaces on the active floor.",
                    )
                    .small()
                    .weak(),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Auto (heuristic)").clicked() {
                        for (layer, cat) in self.layer_map_draft.iter_mut() {
                            *cat = aadt_model::SpaceCategory::from_layer_name(layer);
                        }
                    }
                    if ui.button("All → Walls").clicked() {
                        for cat in self.layer_map_draft.values_mut() {
                            *cat = aadt_model::SpaceCategory::Walls;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        let layers: Vec<String> = self.layer_map_draft.keys().cloned().collect();
                        for layer in layers {
                            let n = counts.get(&layer).copied().unwrap_or(0);
                            let display = layer
                                .strip_prefix(aadt_model::IMPORTED_LAYER_PREFIX)
                                .unwrap_or(&layer);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{display}  ({n})"))
                                        .monospace()
                                        .small(),
                                )
                                .on_hover_text(&layer);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let cat = self
                                            .layer_map_draft
                                            .get(&layer)
                                            .copied()
                                            .unwrap_or(aadt_model::SpaceCategory::Walls);
                                        let mut next = cat;
                                        egui::ComboBox::from_id_salt(format!("lmap_dlg_{layer}"))
                                            .selected_text(cat.as_str())
                                            .width(110.0)
                                            .show_ui(ui, |ui| {
                                                for c in aadt_model::SpaceCategory::ALL {
                                                    if ui
                                                        .selectable_label(cat == c, c.as_str())
                                                        .clicked()
                                                    {
                                                        next = c;
                                                    }
                                                }
                                            });
                                        if next != cat {
                                            self.layer_map_draft.insert(layer.clone(), next);
                                        }
                                    },
                                );
                            });
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Apply map").clicked() {
                        self.apply_layer_map_draft();
                    }
                    if ui.button("Skip").clicked() {
                        self.show_layer_map = false;
                        self.layer_mgr_scope = LayerMgrScope::ActiveSpace;
                    }
                });
            });
        if !open {
            self.show_layer_map = false;
        }
    }
}

fn truncate_ui(s: &str, max_chars: usize) -> String {
    let mut it = s.chars();
    let head: String = it.by_ref().take(max_chars).collect();
    if it.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// Pin scroll/panel content to the current width so wrapped rows reflow on resize.
fn lock_panel_width(ui: &mut egui::Ui) -> f32 {
    let w = ui.available_width().max(1.0);
    ui.set_width(w);
    ui.set_max_width(w);
    w
}

/// Tile edge so a grid of equal cells fills `available` (Fluent spacing gap).
fn fit_tile_size(available: f32, min_tile: f32, max_tile: f32, gap: f32) -> f32 {
    let avail = available.max(1.0);
    let min_t = min_tile.min(avail).max(36.0_f32.min(avail));
    let cols = ((avail + gap) / (min_t + gap)).floor().max(1.0).min(5.0);
    let tile = (avail - gap * (cols - 1.0)) / cols;
    tile.clamp(36.0_f32.min(avail), max_tile)
}

fn caption_chars(tile: f32) -> usize {
    ((tile / 7.0).floor() as usize).clamp(4, 14)
}

fn half_button_width(panel_w: f32) -> f32 {
    ((panel_w - space::SM) * 0.5).max(72.0)
}

fn preview_frame(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    selected: bool,
) -> (egui::Rect, egui::Response) {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    let bg = if selected {
        color::ACCENT_SOFT
    } else {
        color::CANVAS
    };
    let edge = if selected {
        egui::Stroke::new(stroke::THICK, color::ACCENT)
    } else {
        egui::Stroke::new(stroke::THIN, color::BORDER)
    };
    painter.rect_filled(rect, radius::CONTROL, bg);
    painter.rect_stroke(rect, radius::CONTROL, edge);
    (rect, resp)
}

fn hatch_preview_tile(ui: &mut egui::Ui, name: &str, selected: bool, tile: f32) -> egui::Response {
    ui.vertical(|ui| {
        ui.set_width(tile);
        let (rect, resp) = preview_frame(ui, egui::vec2(tile, tile), selected);
        let inset = (tile * 0.09).clamp(3.0, 8.0);
        paint_hatch_swatch(ui.painter(), rect.shrink(inset), name);
        ui.label(
            egui::RichText::new(truncate_ui(name, caption_chars(tile)))
                .small()
                .color(if selected {
                    color::ACCENT
                } else {
                    color::TEXT_SECONDARY
                }),
        );
        resp
    })
    .inner
}

fn paint_hatch_swatch(painter: &egui::Painter, r: egui::Rect, name: &str) {
    let ink = egui::Color32::from_rgb(40, 40, 40);
    let stroke = egui::Stroke::new(1.0_f32, ink);
    let u = name.to_ascii_uppercase();
    painter.rect_stroke(r, 0.0, egui::Stroke::new(0.5_f32, color::STROKE));
    // Clip every pattern to the swatch box: several families (diagonals, tri/net,
    // zigzag) intentionally over-draw the rect and rely on this clip to stay
    // inside their tile instead of bleeding across the panel.
    let painter = painter.with_clip_rect(r);

    if u == "SOLID" {
        painter.rect_filled(r, 0.0, egui::Color32::from_rgb(90, 90, 90));
        return;
    }
    if u.contains("DOT") {
        let step = 7.0_f32;
        let mut y = r.min.y + 4.0;
        while y < r.max.y {
            let mut x = r.min.x + 4.0;
            while x < r.max.x {
                painter.circle_filled(egui::pos2(x, y), 1.2, ink);
                x += step;
            }
            y += step;
        }
        return;
    }
    if u.contains("BR") || u.contains("BRICK") {
        let row_h = 8.0_f32;
        let mut y = r.min.y;
        let mut odd = false;
        while y < r.max.y {
            let offset = if odd { 10.0 } else { 0.0 };
            let mut x = r.min.x - offset;
            while x < r.max.x {
                let brick =
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(18.0, row_h - 1.0));
                if brick.intersects(r) {
                    painter.rect_stroke(brick.intersect(r), 0.0, stroke);
                }
                x += 20.0;
            }
            odd = !odd;
            y += row_h;
        }
        return;
    }
    if u.contains("WOOD") {
        for i in 0..5 {
            let y = r.min.y + 6.0 + i as f32 * 8.0;
            let amp = 2.5_f32;
            let mut prev = egui::pos2(r.min.x, y);
            let mut x = r.min.x;
            while x <= r.max.x {
                let next = egui::pos2(x, y + (x * 0.35).sin() * amp);
                painter.line_segment([prev, next], stroke);
                prev = next;
                x += 3.0;
            }
        }
        return;
    }
    if u.contains("ZIG") {
        let mut pts = Vec::new();
        let mut x = r.min.x;
        let mut up = true;
        while x <= r.max.x + 1.0 {
            let y = if up { r.min.y + 8.0 } else { r.max.y - 8.0 };
            pts.push(egui::pos2(x, y));
            up = !up;
            x += 10.0;
        }
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], stroke);
        }
        return;
    }
    if u.contains("TRIANG") || u.contains("NET3") || u.contains("HONEY") {
        let s = 10.0_f32;
        let mut y = r.min.y;
        let mut row = 0i32;
        while y < r.max.y + s {
            let ox = if row % 2 == 0 { 0.0 } else { s * 0.5 };
            let mut x = r.min.x - s + ox;
            while x < r.max.x + s {
                let a = egui::pos2(x, y + s);
                let b = egui::pos2(x + s * 0.5, y);
                let c = egui::pos2(x + s, y + s);
                painter.line_segment([a, b], stroke);
                painter.line_segment([b, c], stroke);
                painter.line_segment([c, a], stroke);
                x += s;
            }
            row += 1;
            y += s * 0.85;
        }
        return;
    }
    if u.contains("NET") || u.contains("ANSI37") || u.contains("CROSS") {
        let step = 6.0_f32;
        let mut x = r.min.x;
        while x <= r.max.x {
            painter.line_segment([egui::pos2(x, r.min.y), egui::pos2(x, r.max.y)], stroke);
            x += step;
        }
        let mut y = r.min.y;
        while y <= r.max.y {
            painter.line_segment([egui::pos2(r.min.x, y), egui::pos2(r.max.x, y)], stroke);
            y += step;
        }
        return;
    }
    if u.contains("CONC") || u.contains("SAND") || u.contains("GRAVEL") || u.contains("EARTH") {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        u.hash(&mut h);
        let mut seed = h.finish();
        for _ in 0..40 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let fx = ((seed >> 33) as f32) / (u32::MAX as f32);
            let fy = (((seed >> 11) & 0xffff_ffff) as f32) / (u32::MAX as f32);
            let p = egui::pos2(r.min.x + fx * r.width(), r.min.y + fy * r.height());
            painter.circle_filled(p, 1.0, ink);
        }
        return;
    }
    if u.contains("DASH") {
        let step = 7.0_f32;
        let mut y = r.min.y + 4.0;
        while y < r.max.y {
            let mut x = r.min.x + 2.0;
            while x < r.max.x {
                let x2 = (x + 5.0).min(r.max.x);
                painter.line_segment([egui::pos2(x, y), egui::pos2(x2, y)], stroke);
                x += step + 3.0;
            }
            y += step;
        }
        return;
    }
    // Default: ANSI-style diagonal hatch (ANSI31/32/33/34/38/STEEL…).
    let step = if u.contains("STEEL") || u.contains("ANSI32") {
        4.5_f32
    } else {
        6.5_f32
    };
    let mut t = -r.height();
    while t < r.width() + r.height() {
        let a = egui::pos2(r.min.x + t, r.min.y);
        let b = egui::pos2(r.min.x + t + r.height(), r.max.y);
        // Clip-ish by drawing full diagonal across box — painter clips to clip rect.
        painter.line_segment([a, b], stroke);
        t += step;
    }
}

fn block_preview_tile(
    ui: &mut egui::Ui,
    doc: &aadt_model::Document,
    block_name: &str,
    caption: &str,
    selected: bool,
    tile: f32,
) -> egui::Response {
    ui.vertical(|ui| {
        ui.set_width(tile);
        let (rect, resp) = preview_frame(ui, egui::vec2(tile, tile), selected);
        let inset = (tile * 0.1).clamp(4.0, 10.0);
        let inner = rect.shrink(inset);
        if let Some(block) = doc.block(block_name) {
            paint_block_swatch(ui.painter(), inner, block);
        } else {
            paint_placeholder_block(ui.painter(), inner);
        }
        ui.label(
            egui::RichText::new(truncate_ui(caption, caption_chars(tile)))
                .small()
                .color(if selected {
                    color::ACCENT
                } else {
                    color::TEXT_SECONDARY
                }),
        );
        resp
    })
    .inner
}

fn library_preview_tile(
    ui: &mut egui::Ui,
    name: &str,
    path: &std::path::Path,
    selected: bool,
    tile: f32,
) -> egui::Response {
    ui.vertical(|ui| {
        ui.set_width(tile);
        let (rect, resp) = preview_frame(ui, egui::vec2(tile, tile), selected);
        let inset = (tile * 0.12).clamp(5.0, 12.0);
        paint_library_glyph(ui.painter(), rect.shrink(inset), path);
        ui.label(
            egui::RichText::new(truncate_ui(name, caption_chars(tile)))
                .small()
                .color(if selected {
                    color::ACCENT
                } else {
                    color::TEXT_SECONDARY
                }),
        );
        resp
    })
    .inner
}

fn paint_placeholder_block(painter: &egui::Painter, r: egui::Rect) {
    let stroke = egui::Stroke::new(1.2_f32, color::TEXT_SECONDARY);
    painter.rect_stroke(r.shrink(4.0), 0.0, stroke);
    painter.line_segment(
        [
            r.left_top() + egui::vec2(8.0, 8.0),
            r.right_bottom() - egui::vec2(8.0, 8.0),
        ],
        stroke,
    );
}

fn paint_library_glyph(painter: &egui::Painter, r: egui::Rect, path: &std::path::Path) {
    let s = path.to_string_lossy().to_ascii_lowercase();
    let stroke = egui::Stroke::new(1.3_f32, color::TEXT);
    if s.contains("door") {
        let leaf = egui::Rect::from_min_max(
            egui::pos2(r.center().x - 4.0, r.min.y + 4.0),
            egui::pos2(r.max.x - 4.0, r.max.y - 4.0),
        );
        painter.rect_stroke(leaf, 0.0, stroke);
        painter.circle_filled(
            egui::pos2(leaf.min.x + 5.0, leaf.center().y),
            1.5,
            color::TEXT,
        );
        // swing arc
        painter.circle_stroke(
            egui::pos2(leaf.min.x, leaf.max.y),
            leaf.height() * 0.55,
            stroke,
        );
    } else if s.contains("window") {
        painter.rect_stroke(r.shrink(2.0), 0.0, stroke);
        painter.line_segment(
            [
                egui::pos2(r.center().x, r.min.y + 2.0),
                egui::pos2(r.center().x, r.max.y - 2.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(r.min.x + 2.0, r.center().y),
                egui::pos2(r.max.x - 2.0, r.center().y),
            ],
            stroke,
        );
    } else if s.contains("tree") || s.contains("shrub") {
        painter.circle_stroke(r.center(), r.width() * 0.35, stroke);
        painter.line_segment(
            [r.center(), egui::pos2(r.center().x, r.max.y - 2.0)],
            stroke,
        );
    } else if s.contains("furn")
        || s.contains("chair")
        || s.contains("sofa")
        || s.contains("bed")
        || s.contains("desk")
        || s.contains("table")
    {
        painter.rect_stroke(r.shrink(6.0), 0.0, stroke);
        painter.line_segment(
            [
                egui::pos2(r.min.x + 8.0, r.max.y - 6.0),
                egui::pos2(r.min.x + 8.0, r.max.y - 2.0),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(r.max.x - 8.0, r.max.y - 6.0),
                egui::pos2(r.max.x - 8.0, r.max.y - 2.0),
            ],
            stroke,
        );
    } else if s.contains("fixture") || s.contains("toilet") || s.contains("sink") {
        painter.circle_stroke(r.center(), r.width() * 0.28, stroke);
        painter.rect_stroke(
            egui::Rect::from_center_size(r.center() + egui::vec2(0.0, 10.0), egui::vec2(14.0, 8.0)),
            0.0,
            stroke,
        );
    } else {
        paint_placeholder_block(painter, r);
    }
}

fn paint_block_swatch(painter: &egui::Painter, r: egui::Rect, block: &aadt_model::Block) {
    use aadt_geometry::{Bbox, Point2};
    use aadt_model::Geometry;

    let mut pts: Vec<Point2> = Vec::new();
    for be in &block.entities {
        match &be.geometry {
            Geometry::Line(s) => {
                pts.push(s.a);
                pts.push(s.b);
            }
            Geometry::Polyline(p) => pts.extend(p.vertices.iter().copied()),
            Geometry::Circle(c) => {
                pts.push(Point2::new(c.center.x - c.radius, c.center.y - c.radius));
                pts.push(Point2::new(c.center.x + c.radius, c.center.y + c.radius));
            }
            Geometry::Arc(a) => {
                pts.push(Point2::new(a.center.x - a.radius, a.center.y - a.radius));
                pts.push(Point2::new(a.center.x + a.radius, a.center.y + a.radius));
            }
            Geometry::Text(t) => pts.push(t.position),
            Geometry::Insert(i) => pts.push(i.position),
        }
    }
    let Some(bb) = Bbox::of_points(pts) else {
        paint_placeholder_block(painter, r);
        return;
    };
    let bw = bb.width().max(1e-6);
    let bh = bb.height().max(1e-6);
    let scale = (r.width() as f64 / bw).min(r.height() as f64 / bh) * 0.88;
    let cx = (bb.min.x + bb.max.x) * 0.5;
    let cy = (bb.min.y + bb.max.y) * 0.5;
    let to_screen = |p: Point2| {
        egui::pos2(
            r.center().x + ((p.x - cx) * scale) as f32,
            r.center().y - ((p.y - cy) * scale) as f32,
        )
    };
    let stroke = egui::Stroke::new(1.2_f32, color::TEXT);
    for be in &block.entities {
        match &be.geometry {
            Geometry::Line(s) => {
                painter.line_segment([to_screen(s.a), to_screen(s.b)], stroke);
            }
            Geometry::Polyline(p) => {
                for w in p.vertices.windows(2) {
                    painter.line_segment([to_screen(w[0]), to_screen(w[1])], stroke);
                }
                if p.closed && p.vertices.len() > 2 {
                    painter.line_segment(
                        [
                            to_screen(*p.vertices.last().unwrap()),
                            to_screen(p.vertices[0]),
                        ],
                        stroke,
                    );
                }
            }
            Geometry::Circle(c) => {
                painter.circle_stroke(to_screen(c.center), (c.radius * scale) as f32, stroke);
            }
            Geometry::Arc(a) => {
                let sweep = {
                    let mut d = a.end_angle - a.start_angle;
                    while d <= 0.0 {
                        d += 360.0;
                    }
                    d
                };
                let steps = ((sweep / 360.0 * 24.0).ceil() as usize).max(2);
                let mut prev = None;
                for i in 0..=steps {
                    let ang = (a.start_angle + sweep * i as f64 / steps as f64).to_radians();
                    let p = to_screen(Point2::new(
                        a.center.x + a.radius * ang.cos(),
                        a.center.y + a.radius * ang.sin(),
                    ));
                    if let Some(q) = prev {
                        painter.line_segment([q, p], stroke);
                    }
                    prev = Some(p);
                }
            }
            Geometry::Text(t) => {
                painter.circle_filled(to_screen(t.position), 1.5, color::TEXT_SECONDARY);
            }
            Geometry::Insert(i) => {
                let c = to_screen(i.position);
                painter.line_segment(
                    [c + egui::vec2(-3.0, 0.0), c + egui::vec2(3.0, 0.0)],
                    stroke,
                );
                painter.line_segment(
                    [c + egui::vec2(0.0, -3.0), c + egui::vec2(0.0, 3.0)],
                    stroke,
                );
            }
        }
    }
}

fn prop_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let w = ui.available_width();
    if w < 220.0 {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(label).small());
            ui.add(egui::TextEdit::singleline(value).desired_width(ui.available_width()));
        });
    } else {
        ui.horizontal(|ui| {
            let label_w = (w * 0.32).clamp(64.0, 110.0);
            ui.add_sized([label_w, 18.0], egui::Label::new(label));
            ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
        });
    }
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
