//! AutoCAD-like chrome — AAD Chrome 2.0 (paper-first, command-line hero).
//! See [`super::theme`].

use std::time::{Duration, Instant};

use aadt_standards::{aci_to_srgb, LayerSet};
use eframe::egui;

use crate::prefs::PanelMode;

use super::icons::{brand_mark_widget, icon_btn, icon_btn_compact, icon_btn_nav, Icon};
use super::theme::{
    self, bar_frame, button, button_sized, BtnKind, chip_accent, chip_toggle, chrome, color,
    color_swatch, dialog_defaults, dock_frame, float_dock_frame, full_width_button, full_width_edit,
    icon as icon_sz, layout, left_manager_title, link_button, nav_dest_label, NoteKind, note_line,
    panel_space, panel_subtitle, panel_title, prop_row, radius, right_manager_title, section_hint,
    section_label, section_label_first, semibold, space, stroke, tile_frame, two_col_buttons,
    type_scale,
};
use super::{
    block_name_for_category, library_path_for_category, place_elem_tool, place_kinds_for_category,
    tool_for, AadtApp, LayerMgrScope, PlaceStep, Tool,
};

/// How long an auto-hide overlay stays open after the pointer leaves (BricsCAD-like).
const AUTOHIDE_GRACE: Duration = Duration::from_millis(350);

#[derive(Clone, Copy)]
enum LeftRailKind {
    Collapsed,
    AutoHide,
}

#[derive(Clone, Copy)]
enum RightRailKind {
    Collapsed,
    AutoHide,
}

#[derive(Clone, Copy)]
enum PanelChrome {
    Docked,
    Floating,
    AutoHide,
}

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
        ui.horizontal(|ui| {
            self.breadcrumb_nav(ui);
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
            // The active shim layer now lives in the breadcrumb's Layer segment.
            if !self.selection.is_empty()
                && link_button(ui, "To space")
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

    /// Breadcrumb navigation manager for the drafting context —
    /// **Floor › Space › Layer(shim)**. Each segment is a menu that switches the
    /// active floor / model space (or opens the Layer manager), replacing the old
    /// separate Floor/Space combo boxes.
    fn breadcrumb_nav(&mut self, ui: &mut egui::Ui) {
        use aadt_model::SpaceCategory;

        fn space_short_name(name: &str, cat: SpaceCategory) -> String {
            if cat == SpaceCategory::Custom {
                name.rsplit_once('·')
                    .map(|(_, r)| r.trim().to_string())
                    .unwrap_or_else(|| name.to_string())
            } else {
                cat.as_str().to_string()
            }
        }
        fn chevron(ui: &mut egui::Ui) {
            ui.add_space(1.0);
            ui.label(egui::RichText::new("›").color(color::text_secondary()));
            ui.add_space(1.0);
        }

        // Snapshot the active path as owned data so the menus can mutate `self`.
        let active_floor = self.session.document().active_floor_id();
        let floors: Vec<(aadt_model::LevelId, String, u32)> = self
            .session
            .document()
            .floors()
            .into_iter()
            .map(|f| (f.id, f.name.clone(), f.index))
            .collect();
        let floor_name = floors
            .iter()
            .find(|(id, _, _)| Some(*id) == active_floor)
            .map(|(_, n, _)| n.clone())
            .unwrap_or_else(|| "Ground".into());
        let active_space_id = self.session.document().active_space();
        let active_cat = self
            .session
            .document()
            .active_model_space()
            .map(|s| s.category)
            .unwrap_or(SpaceCategory::Walls);
        let space_label = self
            .session
            .document()
            .active_model_space()
            .map(|s| space_short_name(&s.name, s.category))
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
        let shim = self.session.document().active_category_layer();

        // Floor segment.
        ui.menu_button(format!("{floor_name}  ▾"), |ui| {
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
                    ui.close_menu();
                }
            }
            ui.separator();
            if button(ui, BtnKind::Secondary, "＋  Add floor above").clicked() {
                let n = self.session.document().floors().len() + 1;
                let fb = self.app_feed(&format!("FLOORADD Level {n}"));
                if let Some(m) = fb.message() {
                    self.note = m.to_string();
                }
                self.on_floor_or_space_changed();
                ui.close_menu();
            }
        })
        .response
        .on_hover_text("Floor — switch or add");

        chevron(ui);

        // Space segment.
        ui.menu_button(format!("{space_label}  ▾"), |ui| {
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
                    ui.close_menu();
                }
            }
            let customs: Vec<_> = spaces_on_floor
                .iter()
                .filter(|(_, _, c)| *c == SpaceCategory::Custom)
                .collect();
            if !customs.is_empty() {
                ui.separator();
                for (id, name, _) in customs {
                    if ui
                        .selectable_label(
                            *id == active_space_id,
                            space_short_name(name, SpaceCategory::Custom),
                        )
                        .clicked()
                    {
                        let _ = self
                            .session
                            .document_mut()
                            .transact("CATEGORY", |d| d.set_active_space(*id));
                        self.on_floor_or_space_changed();
                        self.note = format!("active model space → {name}");
                        self.dirty = true;
                        ui.close_menu();
                    }
                }
            }
            ui.separator();
            if button(ui, BtnKind::Secondary, "＋  Add custom space").clicked() {
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
                ui.close_menu();
            }
        })
        .response
        .on_hover_text("Model space — switch category / custom space, or add");

        chevron(ui);

        // Layer segment — the derived shim; click to open the Layer manager.
        if ui
            .button(egui::RichText::new(shim).color(color::text_secondary()))
            .on_hover_text(
                "Active shim layer (derived from Floor + Space, ADR-0009). Click to open the Layer manager.",
            )
            .clicked()
        {
            self.left_tab = 0;
            self.reveal_left_mgr();
        }
    }

    pub(super) fn ui_layer_manager(&mut self, ui: &mut egui::Ui) {
        let panel_w = lock_panel_width(ui);
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
                    .font(semibold(type_scale::CAPTION))
                    .color(color::text_secondary()),
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
        // Discipline filter — narrows the list to one HCW discipline (A/S/C/M/…).
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(panel_w);
            ui.spacing_mut().item_spacing.x = space::SM;
            ui.label(
                egui::RichText::new("Discipline")
                    .font(semibold(type_scale::CAPTION))
                    .color(color::text_secondary()),
            );
            let current = self
                .layer_discipline_filter
                .map(|d| d.label())
                .unwrap_or("All");
            egui::ComboBox::from_id_salt("layer_discipline")
                .selected_text(current)
                .width((panel_w * 0.5).clamp(120.0, 200.0))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.layer_discipline_filter.is_none(), "All")
                        .clicked()
                    {
                        self.layer_discipline_filter = None;
                        self.layer_rows_cache = None;
                    }
                    for d in aadt_standards::Discipline::ALL {
                        if ui
                            .selectable_label(self.layer_discipline_filter == Some(*d), d.label())
                            .clicked()
                        {
                            self.layer_discipline_filter = Some(*d);
                            self.layer_rows_cache = None;
                        }
                    }
                });
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
        // AutoCAD Layer Properties Manager: an aligned, striped column grid.
        // Columns follow AutoCAD's order — Status · Name · On · Freeze · Lock ·
        // Color · Linetype · Lineweight. Per-layer actions (set current, delete,
        // map to space) live on the row's right-click context menu.
        let filter = self.layer_filter.trim().to_ascii_uppercase();
        // Bulk On/Thaw/Unlock — the layers listed here, respecting the active
        // scope/discipline/search filter above, exactly as shown in the grid
        // below. Needed to get an imported drawing back to a scalable,
        // editable state in one go: DXF/DWG imports commonly land with some
        // source layers off, frozen, or locked, which silently blocks
        // SCALE/STRETCH/MOVE on everything drawn on them.
        let filtered_names = |rows: &[(String, u8, bool, bool, bool, f32, String)]| -> Vec<String> {
            rows.iter()
                .filter(|(name, ..)| {
                    filter.is_empty() || name.to_ascii_uppercase().contains(&filter)
                })
                .map(|(name, ..)| name.clone())
                .collect::<Vec<_>>()
        };
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(panel_w);
            ui.spacing_mut().item_spacing.x = space::SM;
            ui.label(
                egui::RichText::new("Bulk")
                    .font(semibold(type_scale::CAPTION))
                    .color(color::text_secondary()),
            );
            let hint = format!("{} layers listed below", self.layer_mgr_scope.label());
            if ui
                .button("Show all")
                .on_hover_text(format!("Turn on every {hint}"))
                .clicked()
            {
                let names = filtered_names(&rows);
                for name in &names {
                    self.app_feed(&format!("LAYER {name} ON"));
                }
                self.note = format!("Show all — {} layer(s) turned on", names.len());
            }
            if ui
                .button("Thaw all")
                .on_hover_text(format!("Unfreeze every {hint}"))
                .clicked()
            {
                let names = filtered_names(&rows);
                for name in &names {
                    self.app_feed(&format!("LAYER {name} THAW"));
                }
                self.note = format!("Thaw all — {} layer(s) unfrozen", names.len());
            }
            if ui
                .button("Unlock all")
                .on_hover_text(format!("Unlock every {hint}"))
                .clicked()
            {
                let names = filtered_names(&rows);
                for name in &names {
                    self.app_feed(&format!("LAYER {name} UNLOCK"));
                }
                self.note = format!("Unlock all — {} layer(s) unlocked", names.len());
            }
        });
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("layer_grid")
                .striped(true)
                .spacing([space::SM, space::XS])
                .show(ui, |ui| {
                    let hdr = |ui: &mut egui::Ui, t: &str| {
                        ui.label(
                            egui::RichText::new(t)
                                .font(semibold(type_scale::CAPTION))
                                .color(color::text_secondary()),
                        );
                    };
                    for h in ["St", "Name", "On", "Frz", "Lck", "Color", "LW"] {
                        hdr(ui, h);
                    }
                    ui.end_row();

                    for (name, color_aci, visible, frozen, locked, lw, _lt) in rows {
                        if !filter.is_empty() && !name.to_ascii_uppercase().contains(&filter) {
                            continue;
                        }
                        let is_current = self.current_layer == name || name == shim;
                        let mapped = map_snapshot
                            .get(&name)
                            .copied()
                            .unwrap_or_else(|| aadt_model::SpaceCategory::from_layer_name(&name));

                        // Status — current-layer marker (AutoCAD ▸).
                        if is_current {
                            ui.colored_label(color::accent(), "▸");
                        } else {
                            ui.label(egui::RichText::new("·").color(color::stroke()));
                        }

                        // Name — click selects, double-click sets current, right-click actions.
                        let display = name
                            .strip_prefix(aadt_model::IMPORTED_LAYER_PREFIX)
                            .unwrap_or(&name);
                        // HCW standard metadata (discipline · element · IFC / codes)
                        // when this layer is part of the catalog.
                        let meta = aadt_standards::find_layer(&name)
                            .map(|l| {
                                let mut s = format!(
                                    "\n{} · {}",
                                    l.discipline.label(),
                                    l.element.label()
                                );
                                if !l.ifc().is_empty() {
                                    s.push_str(&format!("  ·  {}", l.ifc()));
                                }
                                let mut codes = Vec::new();
                                if !l.uniformat().is_empty() {
                                    codes.push(format!("Uniformat {}", l.uniformat()));
                                }
                                if !l.masterformat().is_empty() {
                                    codes.push(format!("MF {}", l.masterformat()));
                                }
                                if !codes.is_empty() {
                                    s.push_str(&format!("\n{}", codes.join(" · ")));
                                }
                                s
                            })
                            .unwrap_or_default();
                        let name_resp = ui
                            .selectable_label(is_current, truncate_ui(display, 18))
                            .on_hover_text(format!(
                                "{name}  ·  space: {mapped}{meta}\nDouble-click sets current · right-click for actions"
                            ));
                        if name_resp.double_clicked() {
                            self.current_layer = name.clone();
                            self.note = format!("Current layer → {name}");
                        } else if name_resp.clicked() {
                            self.current_layer = name.clone();
                        }
                        name_resp.context_menu(|ui| {
                            if button(ui, BtnKind::Subtle, "Set current").clicked() {
                                self.current_layer = name.clone();
                                ui.close_menu();
                            }
                            ui.separator();
                            ui.menu_button("Map to space", |ui| {
                                for c in aadt_model::SpaceCategory::ALL {
                                    if ui.selectable_label(mapped == c, c.as_str()).clicked() {
                                        let _ = self.app_feed(&format!("LAYERMAP {name} {}", c.as_str()));
                                        if let Some(floor) = self.session.document().active_floor_id()
                                        {
                                            self.session.document_mut().reassign_by_layer_map(floor);
                                            self.dirty = true;
                                            self.layer_rows_cache = None;
                                        }
                                        ui.close_menu();
                                    }
                                }
                            });
                            if name != "0" && button(ui, BtnKind::Danger, "Delete layer").clicked() {
                                let _ = self.app_feed(&format!("LAYER {name} REMOVE"));
                                if self.current_layer == name {
                                    self.current_layer = "0".into();
                                }
                                ui.close_menu();
                            }
                        });

                        // On / Freeze / Lock toggles.
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

                        // Color swatch — click opens the ACI palette (AutoCAD's
                        // Select Color dialog).
                        let (r, g, b) = aci_to_srgb(color_aci);
                        let tip = format!("ACI {color_aci} — click to change");
                        let sresp =
                            color_swatch(ui, egui::Color32::from_rgb(r, g, b), &tip);
                        let pop_id = ui.make_persistent_id(("laycolor", &name));
                        if sresp.clicked() {
                            ui.memory_mut(|m| m.toggle_popup(pop_id));
                        }
                        egui::popup_below_widget(
                            ui,
                            pop_id,
                            &sresp,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                ui.set_min_width(196.0);
                                ui.label(
                                    egui::RichText::new(format!("Select Color — ACI {color_aci}"))
                                        .font(semibold(type_scale::CAPTION)),
                                );
                                egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                                    ui.horizontal_wrapped(|ui| {
                                        for aci in 1u8..=255 {
                                            let (cr, cg, cb) = aci_to_srgb(aci);
                                            let (cw, _) = ui.allocate_painter(
                                                egui::vec2(12.0, 12.0),
                                                egui::Sense::click(),
                                            );
                                            ui.painter().rect_filled(
                                                cw.rect,
                                                radius::SMALL,
                                                egui::Color32::from_rgb(cr, cg, cb),
                                            );
                                            if aci == color_aci {
                                                ui.painter().rect_stroke(
                                                    cw.rect,
                                                    radius::SMALL,
                                                    egui::Stroke::new(1.5_f32, color::accent()),
                                                );
                                            }
                                            if cw.on_hover_text(format!("ACI {aci}")).clicked() {
                                                let _ = self
                                                    .app_feed(&format!("LAYER {name} COLOR {aci}"));
                                                ui.memory_mut(|m| m.close_popup());
                                            }
                                        }
                                    });
                                });
                            },
                        );

                        // Lineweight.
                        ui.label(
                            egui::RichText::new(format!("{lw:.2}")).size(type_scale::CAPTION),
                        );
                        ui.end_row();
                    }
                });
        });
        ui.separator();
        ui.small(format!(
            "{scope_label}: {total} layers · active {space_hint}"
        ));
        self.ui_layer_metadata(ui);
    }

    /// HCW standard metadata for the current layer (BIM / takeoff / AI schema).
    /// Read-only: the catalog is the source of truth; per-instance values
    /// (material, fire rating, storey) live on the objects, not the layer.
    fn ui_layer_metadata(&mut self, ui: &mut egui::Ui) {
        let name = self.current_layer.clone();
        let Some(l) = aadt_standards::find_layer(&name) else {
            return;
        };
        ui.add_space(space::SM);
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("Metadata — {name}"))
                .font(semibold(type_scale::CAPTION)),
        )
        .default_open(true)
        .show(ui, |ui| {
            let row = |ui: &mut egui::Ui, k: &str, v: &str| {
                if v.is_empty() {
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(k)
                            .size(type_scale::CAPTION)
                            .color(color::text_secondary()),
                    );
                    ui.label(egui::RichText::new(v).size(type_scale::CAPTION));
                });
            };
            row(ui, "Discipline", l.discipline.label());
            row(ui, "Element", l.element.label());
            row(ui, "Phase", l.phase.label());
            row(ui, "IFC class", l.ifc());
            row(ui, "Uniformat", l.uniformat());
            row(ui, "MasterFormat", l.masterformat());
            row(ui, "Plot", if l.plottable { "Plotted" } else { "No plot" });
            let tags = l.ai_tags().join(", ");
            row(ui, "AI tags", &tags);
        });
    }

    pub(super) fn ui_blocks_manager(&mut self, ui: &mut egui::Ui) {
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
                let tile = fit_tile_size(
                    panel_w,
                    panel_space::TILE_MIN_BLOCKS,
                    panel_space::TILE_MAX_BLOCKS,
                    space::SM,
                );
                let half = half_button_width(panel_w);
                // `horizontal_wrapped` under-wraps inside a scroll area (tiles
                // overflow the panel edge and force it to grow), so tile grids
                // chunk into a fixed column count derived from the real width.
                let cols = (((panel_w + space::SM) / (tile + space::SM)).floor() as usize)
                    .clamp(1, 4);

                ui.label(
                    egui::RichText::new(format!("Active space: {}", cat.as_str()))
                        .small()
                        .color(color::accent()),
                );
                ui.add_space(space::XS);

                section_label_first(ui, "Create");
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = space::SM;
                    if button_sized(ui, BtnKind::Secondary, "BLOCK", half)
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
                    if button_sized(ui, BtnKind::Secondary, "WBLOCK", half)
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
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = space::SM;
                    if button_sized(ui, BtnKind::Secondary, "Import PNG…", half)
                        .on_hover_text("Copy PNG/JPEG/WebP into libraries/elements/furniture and place")
                        .clicked()
                    {
                        self.begin_place_image_dialog();
                    }
                    if button_sized(ui, BtnKind::Subtle, "IMAGE", half)
                        .on_hover_text("Place raster image (IMAGE / PLACEIMAGE)")
                        .clicked()
                    {
                        self.begin_place_image_dialog();
                    }
                });

                // Catalog entries with kind badges + place tiles.
                self.ensure_element_catalog(false);
                if !self.element_catalog.is_empty() {
                    section_label(ui, "Catalog");
                    let gap = space::SM;
                    let avail = lock_panel_width(ui);
                    let tile =
                        fit_tile_size(avail, panel_space::TILE_MIN, panel_space::TILE_MAX, gap);
                    let cols = (((avail + gap) / (tile + gap)).floor() as usize).clamp(1, 4);
                    let entries = self.element_catalog.clone();
                    for chunk in entries.chunks(cols) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                            for entry in chunk {
                                let badge = match entry.kind {
                                    aadt_library::ElementKind::Vector => "VEC",
                                    aadt_library::ElementKind::Image => "PNG",
                                    aadt_library::ElementKind::Dynamic => "DYN",
                                };
                                let label = if entry.display_name.is_empty() {
                                    entry.name.clone()
                                } else {
                                    entry.display_name.clone()
                                };
                                let tip = format!(
                                    "{badge} · {} · {} · click to place",
                                    entry.category, entry.path
                                );
                                let resp = ui
                                    .allocate_ui(egui::vec2(tile, tile + 18.0), |ui| {
                                        ui.vertical(|ui| {
                                            ui.set_min_size(egui::vec2(tile, tile));
                                            let selected = self.insert_block == entry.name;
                                            match entry.kind {
                                                aadt_library::ElementKind::Image => {
                                                    ui.centered_and_justified(|ui| {
                                                        ui.label(
                                                            egui::RichText::new("PNG")
                                                                .color(color::accent())
                                                                .strong(),
                                                        );
                                                    });
                                                }
                                                _ => {
                                                    let _ = block_preview_tile(
                                                        ui,
                                                        self.session.document(),
                                                        &entry.name,
                                                        &label,
                                                        selected,
                                                        tile - 4.0,
                                                    );
                                                }
                                            }
                                            ui.label(
                                                egui::RichText::new(format!("{badge} {label}"))
                                                    .small()
                                                    .strong(),
                                            );
                                        });
                                    })
                                    .response
                                    .on_hover_text(tip);
                                if resp.clicked() {
                                    match entry.kind {
                                        aadt_library::ElementKind::Image => {
                                            let root = aadt_library::default_library_root();
                                            let p = root.join(&entry.path);
                                            if p.is_file() {
                                                self.begin_place_image_path(p);
                                            } else {
                                                self.begin_place_image_dialog();
                                            }
                                        }
                                        _ => {
                                            let root = aadt_library::default_library_root();
                                            let p = root.join(&entry.path);
                                            if p.is_file() {
                                                let _ = self.session.feed(&format!(
                                                    "BLOCKLIB import {} {}",
                                                    p.display(),
                                                    entry.name
                                                ));
                                            } else {
                                                // Built-in place blocks (e.g. DOOR-SINGLE).
                                                aadt_command::ensure_place_blocks(
                                                    self.session.document_mut(),
                                                );
                                            }
                                            self.insert_block = entry.name.clone();
                                            self.tool = Tool::Insert {
                                                name: entry.name.clone(),
                                            };
                                            self.note = format!(
                                                "INSERT {} — click to place (grips: flip · width)",
                                                entry.name
                                            );
                                            self.blocks_cache.clear();
                                            self.blocks_cache_rev = None;
                                        }
                                    }
                                }
                            }
                        });
                    }
                }

                // Image elements on disk.
                let root = aadt_library::default_library_root();
                if let Ok(images) = aadt_library::list_image_elements(&root) {
                    if !images.is_empty() {
                        section_label(ui, "Images");
                        for img in images {
                            if link_button(ui, &img.name)
                                .on_hover_text(img.path.display().to_string())
                                .clicked()
                            {
                                self.begin_place_image_path(img.path);
                            }
                        }
                    }
                }

                section_label(ui, "Place");
                let place = place_kinds_for_category(cat);
                if place.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "No place tools for this space — switch category or load library.",
                        )
                        .small()
                        .color(color::text_secondary()),
                    );
                } else {
                    // Seed so place-block previews have geometry.
                    aadt_command::ensure_place_blocks(self.session.document_mut());
                    for chunk in place.chunks(cols) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                            for (label, kind) in chunk {
                                let bname = kind.block_name();
                                let selected = self.insert_block.eq_ignore_ascii_case(bname)
                                    && matches!(self.tool, Tool::PlaceElem { kind: k, .. } if k == *kind);
                                let resp = block_preview_tile(
                                    ui,
                                    self.session.document(),
                                    bname,
                                    label,
                                    selected,
                                    tile,
                                );
                                if resp.clicked() {
                                    aadt_command::ensure_place_blocks(self.session.document_mut());
                                    self.tool = place_elem_tool(*kind);
                                    self.insert_block = bname.to_string();
                                    self.note = match kind.start_step() {
                                        PlaceStep::WallA => format!(
                                            "{bname} — select first wall line to trim"
                                        ),
                                        _ => format!("{bname} — click to place"),
                                    };
                                }
                            }
                        });
                    }
                }

                let blocks: Vec<(String, usize)> = self
                    .blocks_cache
                    .iter()
                    .filter(|(name, _)| block_name_for_category(name, cat))
                    .cloned()
                    .collect();
                section_label(ui, "In drawing");
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("({})", blocks.len()))
                            .small()
                            .color(color::text_secondary()),
                    );
                });
                if blocks.is_empty() {
                    ui.label(
                        egui::RichText::new("No blocks for this space — Place or Load library.")
                            .small()
                            .color(color::text_secondary()),
                    );
                } else {
                    for chunk in blocks.chunks(cols) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                            for (name, n) in chunk {
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
                                .font(semibold(type_scale::CAPTION))
                                .size(type_scale::CAPTION),
                        );
                        for (k, v) in &attrs {
                            ui.horizontal_wrapped(|ui| {
                                let label_w = (panel_w * 0.28).clamp(56.0, 88.0);
                                ui.add_sized(
                                    [label_w, panel_space::LABEL_H],
                                    egui::Label::new(
                                        egui::RichText::new(truncate_ui(k, 10))
                                            .size(type_scale::CAPTION)
                                            .color(color::text_secondary()),
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
                                let current = attrs
                                    .iter()
                                    .find(|(k, _)| k.eq_ignore_ascii_case(aadt_model::BLOCK_VIS_ATTR))
                                    .map(|(_, v)| v.clone())
                                    .unwrap_or_else(|| "Standard".into());
                                egui::ComboBox::from_id_salt(format!("vis-{}", id.0))
                                    .selected_text(&current)
                                    .width((panel_w * 0.55).clamp(80.0, 160.0))
                                    .show_ui(ui, |ui| {
                                        for st in &states {
                                            if ui
                                                .selectable_label(current.eq_ignore_ascii_case(st), st)
                                                .clicked()
                                            {
                                                let fb = self
                                                    .session
                                                    .feed(&format!("BSETVIS {} {}", id.0, st));
                                                if let Some(m) = fb.message() {
                                                    self.note = m.to_string();
                                                }
                                            }
                                        }
                                    });
                            });
                        }
                        // Flip + Width for dynamic-lite elements.
                        let flipped = attrs
                            .iter()
                            .find(|(k, _)| {
                                k.eq_ignore_ascii_case(aadt_model::BLOCK_FLIP_ATTR)
                                    || k.eq_ignore_ascii_case(aadt_model::BLOCK_FLIP_PARAM)
                            })
                            .map(|(_, v)| {
                                v == "1"
                                    || v.eq_ignore_ascii_case("true")
                                    || v.eq_ignore_ascii_case("yes")
                            })
                            .unwrap_or(false);
                        let mut flip = flipped;
                        if ui.checkbox(&mut flip, "Flip").changed() {
                            let fb = self.session.feed(&format!("BFLIP {}", id.0));
                            if let Some(m) = fb.message() {
                                self.note = m.to_string();
                            }
                        }
                        let width_val = attrs
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(aadt_model::BLOCK_WIDTH_PARAM))
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.small("Width");
                            let mut edit = width_val;
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut edit)
                                        .desired_width(72.0)
                                        .clip_text(true),
                                )
                                .changed()
                                && !edit.is_empty()
                            {
                                let fb = self.session.feed(&format!(
                                    "EATTEDIT {} {} {}",
                                    id.0,
                                    aadt_model::BLOCK_WIDTH_PARAM,
                                    edit
                                ));
                                if let Some(m) = fb.message() {
                                    self.note = m.to_string();
                                }
                            }
                        });
                    }
                }

                section_label(ui, "Library");
                ui.label(
                    egui::RichText::new(format!(
                        "Filtered to {} — Load scales inches → metres.",
                        cat.as_str()
                    ))
                    .small()
                    .color(color::text_secondary()),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = space::SM;
                    if button_sized(ui, BtnKind::Secondary, "Load matching", half)
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
                    if button_sized(ui, BtnKind::Secondary, "Seed place", half).clicked() {
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
                            .color(color::text_secondary()),
                    );
                    for chunk in lib.chunks(cols) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                            for (name, path) in chunk {
                                let selected = self.insert_block.eq_ignore_ascii_case(name);
                                // Prefer document geometry if imported; else folder glyph.
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
                                    let fb =
                                        self.session.feed(&format!("BLOCKLIB import {p} {name}"));
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
                }

                if !self.insert_block.is_empty() {
                    ui.add_space(space::XS);
                    ui.label(
                        egui::RichText::new(format!("Active: {}", self.insert_block))
                            .small()
                            .color(color::accent()),
                    );
                }
            });
    }

    pub(super) fn ui_selection_properties(&mut self, ui: &mut egui::Ui) {
        use aadt_model::Geometry;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let panel_w = lock_panel_width(ui);
            section_label_first(ui, "Display");
            if ui
                .checkbox(&mut self.show_lweights, "Lineweights in viewport (LWT)")
                .on_hover_text("Scale layer plot lineweights to screen pixels")
                .changed()
            {
                self.persist_prefs();
            }
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                if ui
                    .selectable_label(
                        self.view_preset == super::ViewPreset::TechnicalPlan,
                        "Technical",
                    )
                    .clicked()
                {
                    self.apply_view_preset(super::ViewPreset::TechnicalPlan);
                }
                if ui
                    .selectable_label(
                        self.view_preset == super::ViewPreset::Presentation,
                        "Presentation",
                    )
                    .clicked()
                {
                    self.apply_view_preset(super::ViewPreset::Presentation);
                }
            });
            ui.label(
                egui::RichText::new(format!(
                    "Active preset: {} · Fills: Technical / Floor / Presentation in Fills dock",
                    self.view_preset.label()
                ))
                .small()
                .weak(),
            );
            ui.add_space(panel_space::ROW);

            if self.selection.is_empty() && self.selected_hatches.is_empty() {
                ui.label(
                    egui::RichText::new("No selection")
                        .font(semibold(type_scale::BODY))
                        .color(color::text_secondary()),
                );
                ui.label(
                    egui::RichText::new(
                        "Click objects or a fill in the active Floor + Space. Del erases entities, fills, and dimensions together.",
                    )
                    .small()
                    .weak(),
                );
                return;
            }
            if !self.selected_hatches.is_empty() {
                ui.label(
                    egui::RichText::new(format!(
                        "{} fill(s) selected",
                        self.selected_hatches.len()
                    ))
                    .font(semibold(type_scale::BODY)),
                );
                ui.label(
                    egui::RichText::new("Del/E erases fills · Fills dock retargets pattern")
                        .small()
                        .weak(),
                );
                ui.add_space(space::SM);
            }
            if self.selection.is_empty() {
                return;
            }
            ui.label(
                egui::RichText::new(format!("{} object(s)", self.selection.len()))
                    .font(semibold(type_scale::BODY)),
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
                    Geometry::Image(_) => "IMAGE",
                    Geometry::Insert(i) => {
                        ui.group(|ui| {
                            ui.label(egui::RichText::new(format!("#{} INSERT", id.0)).font(semibold(type_scale::BODY)));
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
                    ui.label(egui::RichText::new(format!("#{} {kind}", id.0)).font(semibold(type_scale::BODY)));
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
                if button(ui, BtnKind::Secondary, "Move to active space")
                    .on_hover_text("CHSPACE — move selection into the active model space")
                    .clicked()
                {
                    self.sync_selection();
                    let fb = self.app_feed("CHSPACE");
                    if let Some(m) = fb.message() {
                        self.note = m.to_string();
                    }
                }
                if button(ui, BtnKind::Danger, "Erase").clicked() {
                    self.erase_selection();
                }
            });
        });
    }

    pub(super) fn ui_history_panel(&mut self, ui: &mut egui::Ui) {
        let _ = lock_panel_width(ui);
        section_label_first(ui, "Versions");
        section_hint(ui, "File checkpoints inside .aadt — not the same as UNDO.");
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
                                if link_button(ui, "Restore").clicked() {
                                    self.app_feed(&format!("HISTORY RESTORE {}", c.id));
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
                                .color(color::text_secondary()),
                            );
                            ui.add_space(panel_space::TIGHT);
                        }
                    }
                    Err(e) => {
                        ui.colored_label(color::danger(), format!("History: {e}"));
                    }
                }
            } else {
                section_hint(ui, "Save as .aadt to enable the time machine.");
            }
        } else {
            section_hint(ui, "OPEN or SAVE a .aadt drawing to use History.");
        }
    }

    /// Top strip — Chrome 2.0: one paper-first bar (file tools + floor context).
    pub(super) fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("qat")
            .exact_height(chrome::TOP_BAR_H)
            .frame(
                egui::Frame::none()
                    .fill(color::layer_card())
                    .inner_margin(egui::Margin::symmetric(space::SM, space::XS))
                    .stroke(egui::Stroke::new(stroke::THIN, color::stroke())),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = space::XS;
                    brand_mark_widget(ui, icon_sz::BRAND);
                    ui.label(
                        egui::RichText::new("AAD")
                            .font(semibold(type_scale::BODY))
                            .color(color::text()),
                    );
                    ui.add_space(space::SM);
                    ui.separator();
                    ui.add_space(space::SM);
                    if icon_btn(ui, Icon::New, "New drawing (Ctrl+N)").clicked() {
                        self.request_new_drawing();
                    }
                    if icon_btn(ui, Icon::Open, "Open… (Ctrl+O)").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("AAD / DXF / DWG", &["aadt", "dxf", "dwg"])
                            .add_filter("DWG", &["dwg"])
                            .add_filter("DXF", &["dxf"])
                            .add_filter("AAD", &["aadt"])
                            .pick_file()
                        {
                            self.request_open_path(path);
                        }
                    }
                    if icon_btn(ui, Icon::Save, "Save (Ctrl+S)").clicked() {
                        self.quick_save();
                    }
                    ui.add_space(space::SM);
                    if icon_btn(ui, Icon::Undo, "Undo (Ctrl+Z)").clicked() {
                        self.app_feed("UNDO");
                    }
                    if icon_btn(ui, Icon::Redo, "Redo (Ctrl+Y)").clicked() {
                        self.app_feed("REDO");
                    }
                    ui.add_space(space::SM);
                    if icon_btn(ui, Icon::Plot, "Plot to PDF… (Ctrl+P)").clicked() {
                        self.seed_plot_dialog();
                    }
                    ui.menu_button("⋯", |ui| {
                        if ui
                            .button("Server…")
                            .on_hover_text("Connect to a ShilpiDB (shilpid) server")
                            .clicked()
                        {
                            self.show_server = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.label(egui::RichText::new("View").small().weak());
                        if ui
                            .selectable_label(
                                self.view_preset == super::ViewPreset::TechnicalPlan,
                                "Technical plan",
                            )
                            .clicked()
                        {
                            self.apply_view_preset(super::ViewPreset::TechnicalPlan);
                            ui.close_menu();
                        }
                        if ui
                            .selectable_label(
                                self.view_preset == super::ViewPreset::Presentation,
                                "Presentation",
                            )
                            .clicked()
                        {
                            self.apply_view_preset(super::ViewPreset::Presentation);
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.label(egui::RichText::new("Layer groups").small().weak());
                        for group in super::LayerGroup::ALL {
                            let hidden = self.layer_group_hidden(group);
                            if ui
                                .selectable_label(!hidden, group.label())
                                .on_hover_text(if hidden {
                                    "Hidden — click to show"
                                } else {
                                    "Shown — click to hide"
                                })
                                .clicked()
                            {
                                self.toggle_layer_group(group);
                            }
                        }
                        ui.separator();
                        if ui.button("Keyboard help (F1)").clicked() {
                            self.show_shortcuts = true;
                            ui.close_menu();
                        }
                    });
                    ui.add_space(space::MD);
                    ui.separator();
                    ui.add_space(space::SM);
                    self.ui_levels_strip(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let icon = if self.theme.is_dark() {
                            Icon::Sun
                        } else {
                            Icon::Moon
                        };
                        let tip = format!(
                            "Theme: {} — switch to {}",
                            self.theme.label(),
                            self.theme.toggled().label()
                        );
                        if icon_btn(ui, icon, &tip).clicked() {
                            let next = self.theme.toggled();
                            let ctx = ui.ctx().clone();
                            self.set_theme(&ctx, next);
                        }
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
            .min_height(120.0)
            .max_height(layout::drawing_props_max_h(ctx.screen_rect()))
            .resizable(true)
            .frame(
                egui::Frame::none()
                    .fill(color::layer_card())
                    .inner_margin(egui::Margin::same(space::MD))
                    .stroke(egui::Stroke::new(stroke::THIN, color::border())),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Drawing properties")
                            .heading()
                            .color(color::text()),
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
        self.sync_cli_log_from_note();
        let show_hint = self.tool_prompt().is_some() || !self.note.is_empty();
        let token = self.command.trim().to_string();
        let completions = if !token.is_empty() && !token.contains(' ') {
            self.session.complete(&token)
        } else {
            Vec::new()
        };
        let show_comp = !completions.is_empty();
        let bar_h = if show_hint || show_comp {
            chrome::STATUS_H_EXP
        } else {
            chrome::STATUS_H
        };

        egui::TopBottomPanel::bottom("status")
            .exact_height(bar_h)
            .frame(bar_frame())
            .show(ctx, |ui| {
                // CLI history (Chrome 2.0 hero) — newest at the bottom of the stack.
                let hist: Vec<String> = self
                    .cli_log
                    .iter()
                    .rev()
                    .take(chrome::CLI_HISTORY_LINES)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                ui.vertical(|ui| {
                    ui.set_min_height(
                        (chrome::CLI_HISTORY_LINES as f32) * (type_scale::CAPTION + 2.0),
                    );
                    for line in &hist {
                        ui.label(
                            egui::RichText::new(truncate_ui(line, 120))
                                .size(type_scale::CAPTION)
                                .color(color::text_secondary()),
                        );
                    }
                    if hist.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Command line — type a command · Space runs · F1 help",
                            )
                            .size(type_scale::CAPTION)
                            .color(color::text_disabled()),
                        );
                    }
                });

                // Command input + status rail.
                ui.horizontal(|ui| {
                    let units =
                        aadt_model::primary_from_props(&self.session.document().properties().units);
                    if chip_accent(
                        ui,
                        units.as_token(),
                        &format!("{} — open Units manager", units.label()),
                    )
                    .clicked()
                    {
                        self.right_tab = 4;
                        self.props_edit = self.session.document().properties().clone();
                        self.reveal_right_mgr();
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
                    if icon_btn_nav(ui, Icon::ObjectProps, "Properties panel", self.right_tab == 7)
                        .clicked()
                    {
                        self.right_tab = 7;
                        self.reveal_right_mgr();
                    }
                    ui.separator();

                    // Keep command field proportional so narrow windows don't overflow.
                    let avail = ui.available_width();
                    let status_reserve = (avail * 0.38).clamp(140.0, 380.0);
                    let cmd_w = (avail - status_reserve).max(120.0);
                    // Capture before TextEdit borrows `self.command`.
                    let cmd_interactive = !self.drafting();
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.command)
                            .desired_width(cmd_w)
                            .hint_text("Type a command — Space runs · F1 help")
                            // While drafting, keep Tab/Space away from this field —
                            // focus here made Tab then Space fire chrome actions
                            // (including closing the app).
                            .interactive(cmd_interactive),
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
                                self.push_cli_log(format!("Command: {token}"));
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
                                .color(color::text_secondary()),
                        );
                        if self.dirty {
                            ui.colored_label(color::warning(), "MOD");
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
                                    .color(color::text_secondary()),
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
                            if chip_toggle(ui, label, on, tip).clicked() {
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
                                    .checkbox(&mut self.show_lweights, "Lineweights (LWT)")
                                    .changed()
                                {
                                    self.persist_prefs();
                                }
                                ui.separator();
                                ui.label(egui::RichText::new("Display").small().strong());
                                if ui
                                    .selectable_label(
                                        self.view_preset == super::ViewPreset::TechnicalPlan,
                                        "Technical plan",
                                    )
                                    .on_hover_text(
                                        "Show technical/floor fills; mute presentation layers",
                                    )
                                    .clicked()
                                {
                                    self.apply_view_preset(super::ViewPreset::TechnicalPlan);
                                }
                                if ui
                                    .selectable_label(
                                        self.view_preset == super::ViewPreset::Presentation,
                                        "Presentation board",
                                    )
                                    .on_hover_text(
                                        "Show presentation fills + PNG furniture; soft wash alpha",
                                    )
                                    .clicked()
                                {
                                    self.apply_view_preset(super::ViewPreset::Presentation);
                                }
                                ui.label(
                                    egui::RichText::new(
                                        "LWT scales layer lineweights in the viewport (plot mm → screen px).",
                                    )
                                    .small()
                                    .weak(),
                                );
                                if button(ui, BtnKind::Subtle, "Keyboard help (F1)").clicked() {
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
                            if link_button(ui, m).clicked() {
                                let up = m.to_ascii_uppercase();
                                if matches!(up.as_str(), "E" | "ERASE")
                                    || tool_for(m, !self.selection.is_empty()).is_some()
                                {
                                    self.command.clear();
                                    self.last_line = Some(m.clone());
                                    self.push_cli_log(format!("Command: {m}"));
                                    self.dispatch(m);
                                } else {
                                    self.command = format!("{m} ");
                                    self.refocus = true;
                                }
                            }
                        }
                        if let Some(prompt) = self.tool_prompt() {
                            let short = truncate_ui(prompt, 72);
                            note_line(ui, NoteKind::Prompt, short);
                        } else if !self.note.is_empty() {
                            let short = truncate_ui(&self.note, 72);
                            note_line(ui, NoteKind::Info, short);
                        }
                    });
                }
            });
    }

    pub(super) fn draw_left_panel(&mut self, ctx: &egui::Context) {
        match self.left_panel_mode {
            PanelMode::Floating => self.draw_left_floating(ctx),
            PanelMode::AutoHide => self.draw_left_autohide(ctx),
            PanelMode::Docked => {
                if self.show_left_mgr {
                    self.draw_left_docked(ctx);
                } else {
                    self.draw_left_rail(ctx, LeftRailKind::Collapsed);
                }
            }
        }
    }

    pub(super) fn draw_right_panel(&mut self, ctx: &egui::Context) {
        match self.right_panel_mode {
            PanelMode::Floating => self.draw_right_floating(ctx),
            PanelMode::AutoHide => self.draw_right_autohide(ctx),
            PanelMode::Docked => {
                if self.show_layer_mgr {
                    self.draw_right_docked(ctx);
                } else {
                    self.draw_right_rail(ctx, RightRailKind::Collapsed);
                }
            }
        }
    }

    /// Reveal the left manager in whatever mode it is currently in.
    pub(super) fn reveal_left_mgr(&mut self) {
        match self.left_panel_mode {
            PanelMode::Docked | PanelMode::Floating => {
                self.show_left_mgr = true;
            }
            PanelMode::AutoHide => {
                self.left_autohide_open = true;
                self.left_autohide_until = Some(Instant::now() + AUTOHIDE_GRACE);
            }
        }
        self.persist_prefs();
    }

    /// Reveal the right manager in whatever mode it is currently in.
    pub(super) fn reveal_right_mgr(&mut self) {
        match self.right_panel_mode {
            PanelMode::Docked | PanelMode::Floating => {
                self.show_layer_mgr = true;
            }
            PanelMode::AutoHide => {
                self.right_autohide_open = true;
                self.right_autohide_until = Some(Instant::now() + AUTOHIDE_GRACE);
            }
        }
        self.persist_prefs();
    }

    fn set_left_panel_mode(&mut self, mode: PanelMode) {
        self.left_panel_mode = mode;
        match mode {
            PanelMode::Docked => {
                self.show_left_mgr = true;
                self.left_autohide_open = false;
                self.left_autohide_until = None;
            }
            PanelMode::Floating => {
                self.show_left_mgr = true;
                self.left_autohide_open = false;
                self.left_autohide_until = None;
            }
            PanelMode::AutoHide => {
                self.show_left_mgr = false;
                self.left_autohide_open = false;
                self.left_autohide_until = None;
            }
        }
        self.persist_prefs();
    }

    fn set_right_panel_mode(&mut self, mode: PanelMode) {
        self.right_panel_mode = mode;
        match mode {
            PanelMode::Docked => {
                self.show_layer_mgr = true;
                self.right_autohide_open = false;
                self.right_autohide_until = None;
            }
            PanelMode::Floating => {
                self.show_layer_mgr = true;
                self.right_autohide_open = false;
                self.right_autohide_until = None;
            }
            PanelMode::AutoHide => {
                self.show_layer_mgr = false;
                self.right_autohide_open = false;
                self.right_autohide_until = None;
            }
        }
        self.persist_prefs();
    }

    fn draw_left_rail(&mut self, ctx: &egui::Context, kind: LeftRailKind) {
        let rail = egui::SidePanel::left("left_mgr_collapsed")
            .exact_width(panel_space::RAIL)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(color::layer_base())
                    .stroke(egui::Stroke::new(stroke::THIN, color::border())),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(space::SM);
                    let expand_tip = match kind {
                        LeftRailKind::Collapsed => "Expand structure",
                        LeftRailKind::AutoHide => "Hover to show structure (auto-hide)",
                    };
                    if icon_btn_nav(ui, Icon::ExpandPanel, expand_tip, false).clicked() {
                        match kind {
                            LeftRailKind::Collapsed => {
                                self.show_left_mgr = true;
                                self.persist_prefs();
                            }
                            LeftRailKind::AutoHide => {
                                self.left_autohide_open = true;
                                self.left_autohide_until = Some(Instant::now() + AUTOHIDE_GRACE);
                            }
                        }
                    }
                    ui.add_space(space::MD);
                    for dest in theme::left_manager_nav() {
                        let tip = format!("{} — {}", dest.label, dest.tip);
                        if icon_btn_nav(ui, dest.icon, &tip, self.left_tab == dest.tab).clicked() {
                            self.left_tab = dest.tab;
                            match kind {
                                LeftRailKind::Collapsed => {
                                    self.show_left_mgr = true;
                                    self.persist_prefs();
                                }
                                LeftRailKind::AutoHide => {
                                    self.left_autohide_open = true;
                                    self.left_autohide_until =
                                        Some(Instant::now() + AUTOHIDE_GRACE);
                                    self.persist_prefs();
                                }
                            }
                        }
                    }
                });
            });
        if matches!(kind, LeftRailKind::AutoHide) && rail.response.hovered() {
            self.left_autohide_open = true;
            self.left_autohide_until = Some(Instant::now() + AUTOHIDE_GRACE);
            ctx.request_repaint();
        }
    }

    fn draw_right_rail(&mut self, ctx: &egui::Context, kind: RightRailKind) {
        let rail = egui::SidePanel::right("layer_mgr_collapsed")
            .exact_width(panel_space::RAIL)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(color::layer_base())
                    .stroke(egui::Stroke::new(stroke::THIN, color::border())),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(space::SM);
                    let expand_tip = match kind {
                        RightRailKind::Collapsed => "Expand tools",
                        RightRailKind::AutoHide => "Hover to show tools (auto-hide)",
                    };
                    if icon_btn_nav(ui, Icon::ExpandPanel, expand_tip, false).clicked() {
                        match kind {
                            RightRailKind::Collapsed => {
                                self.show_layer_mgr = true;
                                self.persist_prefs();
                            }
                            RightRailKind::AutoHide => {
                                self.right_autohide_open = true;
                                self.right_autohide_until = Some(Instant::now() + AUTOHIDE_GRACE);
                            }
                        }
                    }
                    ui.add_space(space::MD);
                    for dest in theme::right_manager_nav() {
                        let tip = format!("{} — {}", dest.label, dest.tip);
                        if icon_btn_nav(ui, dest.icon, &tip, self.right_tab == dest.tab).clicked() {
                            self.right_tab = dest.tab;
                            if matches!(dest.tab, 4 | 5) {
                                self.props_edit = self.session.document().properties().clone();
                            }
                            match kind {
                                RightRailKind::Collapsed => {
                                    self.show_layer_mgr = true;
                                    self.persist_prefs();
                                }
                                RightRailKind::AutoHide => {
                                    self.right_autohide_open = true;
                                    self.right_autohide_until =
                                        Some(Instant::now() + AUTOHIDE_GRACE);
                                    self.persist_prefs();
                                }
                            }
                        }
                    }
                });
            });
        if matches!(kind, RightRailKind::AutoHide) && rail.response.hovered() {
            self.right_autohide_open = true;
            self.right_autohide_until = Some(Instant::now() + AUTOHIDE_GRACE);
            ctx.request_repaint();
        }
    }

    fn left_panel_chrome_buttons(&mut self, ui: &mut egui::Ui, placement: PanelChrome) {
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match placement {
                PanelChrome::Docked => {
                    if icon_btn(ui, Icon::CollapsePanel, "Collapse to rail").clicked() {
                        self.show_left_mgr = false;
                        self.persist_prefs();
                    }
                    if icon_btn(ui, Icon::UnpinPanel, "Auto-hide (expand on hover)").clicked() {
                        self.set_left_panel_mode(PanelMode::AutoHide);
                    }
                    if icon_btn(ui, Icon::FloatPanel, "Float panel").clicked() {
                        self.set_left_panel_mode(PanelMode::Floating);
                    }
                }
                PanelChrome::Floating => {
                    if icon_btn(ui, Icon::UnpinPanel, "Auto-hide (expand on hover)").clicked() {
                        self.set_left_panel_mode(PanelMode::AutoHide);
                    }
                    if icon_btn(ui, Icon::DockPanel, "Dock to left edge").clicked() {
                        self.set_left_panel_mode(PanelMode::Docked);
                    }
                }
                PanelChrome::AutoHide => {
                    if icon_btn(ui, Icon::FloatPanel, "Float panel").clicked() {
                        self.set_left_panel_mode(PanelMode::Floating);
                    }
                    if icon_btn(ui, Icon::PinPanel, "Pin (keep docked)").clicked() {
                        self.set_left_panel_mode(PanelMode::Docked);
                    }
                }
            },
        );
    }

    fn right_panel_chrome_buttons(&mut self, ui: &mut egui::Ui, placement: PanelChrome) {
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| match placement {
                PanelChrome::Docked => {
                    if icon_btn(ui, Icon::CollapsePanel, "Collapse to rail").clicked() {
                        self.show_layer_mgr = false;
                        self.persist_prefs();
                    }
                    if icon_btn(ui, Icon::UnpinPanel, "Auto-hide (expand on hover)").clicked() {
                        self.set_right_panel_mode(PanelMode::AutoHide);
                    }
                    if icon_btn(ui, Icon::FloatPanel, "Float panel").clicked() {
                        self.set_right_panel_mode(PanelMode::Floating);
                    }
                }
                PanelChrome::Floating => {
                    if icon_btn(ui, Icon::UnpinPanel, "Auto-hide (expand on hover)").clicked() {
                        self.set_right_panel_mode(PanelMode::AutoHide);
                    }
                    if icon_btn(ui, Icon::DockPanel, "Dock to right edge").clicked() {
                        self.set_right_panel_mode(PanelMode::Docked);
                    }
                }
                PanelChrome::AutoHide => {
                    if icon_btn(ui, Icon::FloatPanel, "Float panel").clicked() {
                        self.set_right_panel_mode(PanelMode::Floating);
                    }
                    if icon_btn(ui, Icon::PinPanel, "Pin (keep docked)").clicked() {
                        self.set_right_panel_mode(PanelMode::Docked);
                    }
                }
            },
        );
    }

    fn ui_left_panel_body(&mut self, ui: &mut egui::Ui, chrome: PanelChrome) {
        ui.horizontal(|ui| {
            panel_title(ui, left_manager_title(self.left_tab));
            self.left_panel_chrome_buttons(ui, chrome);
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
                let selected = self.left_tab == dest.tab;
                if icon_btn_nav(ui, dest.icon, &tip, selected).clicked() {
                    self.left_tab = dest.tab;
                    self.persist_prefs();
                }
                nav_dest_label(ui, dest.label, selected);
            }
        });
        ui.add_space(panel_space::ROW);
        ui.separator();
        ui.add_space(panel_space::ROW);
        match self.left_tab {
            1 => self.ui_blocks_manager(ui),
            _ => self.ui_layer_manager(ui),
        }
    }

    fn ui_right_panel_body(&mut self, ui: &mut egui::Ui, chrome: PanelChrome) {
        ui.horizontal(|ui| {
            panel_title(ui, right_manager_title(self.right_tab));
            self.right_panel_chrome_buttons(ui, chrome);
        });
        panel_subtitle(
            ui,
            match self.right_tab {
                0 => "Quantities, publish, Ask, classify",
                1 => "Technical · Floor · Presentation fills",
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
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
            for dest in theme::right_manager_nav() {
                let tip = format!("{} — {}", dest.label, dest.tip);
                let selected = self.right_tab == dest.tab;
                if icon_btn_nav(ui, dest.icon, &tip, selected).clicked() {
                    self.right_tab = dest.tab;
                    if matches!(dest.tab, 4 | 5) {
                        self.props_edit = self.session.document().properties().clone();
                    }
                    self.persist_prefs();
                }
                nav_dest_label(ui, dest.label, selected);
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
    }

    fn draw_left_docked(&mut self, ctx: &egui::Context) {
        // exact_width forces the reservation to a fixed width regardless of the
        // content's minimum — otherwise a non-wrapping tile row can make egui
        // reserve far more than the frame fills, leaving a gap beside the dock.
        let both = matches!(self.right_panel_mode, PanelMode::Docked) && self.show_layer_mgr;
        let dock_w = layout::dock_width(320.0, ctx.screen_rect().width(), both);
        let left_resp = egui::SidePanel::left("left_mgr")
            .exact_width(dock_w)
            .resizable(false)
            .frame(dock_frame())
            .show(ctx, |ui| {
                self.ui_left_panel_body(ui, PanelChrome::Docked);
            });
        let pw = left_resp.response.rect.width();
        if (pw - self.left_panel_width).abs() > 1.0 {
            self.left_panel_width = pw.clamp(180.0, 340.0);
        }
    }

    fn draw_right_docked(&mut self, ctx: &egui::Context) {
        let both = matches!(self.left_panel_mode, PanelMode::Docked) && self.show_left_mgr;
        let dock_w = layout::dock_width(324.0, ctx.screen_rect().width(), both);
        let right_resp = egui::SidePanel::right("layer_mgr")
            .exact_width(dock_w)
            .resizable(false)
            .frame(dock_frame())
            .show(ctx, |ui| {
                self.ui_right_panel_body(ui, PanelChrome::Docked);
            });
        let pw = right_resp.response.rect.width();
        if (pw - self.right_panel_width).abs() > 1.0 {
            self.right_panel_width = pw.clamp(200.0, 340.0);
        }
    }

    fn draw_left_floating(&mut self, ctx: &egui::Context) {
        // Thin reopen rail when the floating window is closed.
        if !self.show_left_mgr {
            self.draw_left_rail(ctx, LeftRailKind::Collapsed);
            return;
        }
        let mut open = true;
        let screen = ctx.screen_rect();
        let mut x = self.left_float.x;
        let mut y = self.left_float.y;
        let mut w = self.left_float.w;
        let mut h = self.left_float.h;
        layout::clamp_float_rect(&mut x, &mut y, &mut w, &mut h, screen);
        let mut persist = false;
        egui::Window::new("Structure")
            .id(egui::Id::new("left_mgr_float"))
            .open(&mut open)
            .collapsible(false)
            .default_pos(egui::pos2(x, y))
            .default_size([w, h])
            .min_width(220.0)
            .min_height(160.0)
            .max_height(layout::overlay_height(screen, chrome::STATUS_H))
            .constrain_to(screen)
            .frame(float_dock_frame(egui::vec2(0.0, 2.0)))
            .show(ctx, |ui| {
                self.ui_left_panel_body(ui, PanelChrome::Floating);
            });
        if let Some((rx, ry, rw, rh)) = floating_window_rect(ctx, egui::Id::new("left_mgr_float")) {
            let mut nx = rx;
            let mut ny = ry;
            let mut nw = rw;
            let mut nh = rh;
            layout::clamp_float_rect(&mut nx, &mut ny, &mut nw, &mut nh, screen);
            if (nx - self.left_float.x).abs() > 1.0
                || (ny - self.left_float.y).abs() > 1.0
                || (nw - self.left_float.w).abs() > 1.0
                || (nh - self.left_float.h).abs() > 1.0
            {
                self.left_float.x = nx;
                self.left_float.y = ny;
                self.left_float.w = nw;
                self.left_float.h = nh;
                persist = true;
            }
        }
        if !open {
            self.show_left_mgr = false;
            persist = true;
        }
        if persist {
            self.persist_prefs();
        }
    }

    fn draw_right_floating(&mut self, ctx: &egui::Context) {
        if !self.show_layer_mgr {
            self.draw_right_rail(ctx, RightRailKind::Collapsed);
            return;
        }
        let mut open = true;
        let screen = ctx.screen_rect();
        let mut x = self.right_float.x;
        let mut y = self.right_float.y;
        let mut w = self.right_float.w;
        let mut h = self.right_float.h;
        layout::clamp_float_rect(&mut x, &mut y, &mut w, &mut h, screen);
        let mut persist = false;
        egui::Window::new("Tools")
            .id(egui::Id::new("right_mgr_float"))
            .open(&mut open)
            .title_bar(false)
            .collapsible(false)
            .default_pos(egui::pos2(x, y))
            .default_size([w, h])
            .min_width(220.0)
            .min_height(160.0)
            .max_height(layout::overlay_height(screen, chrome::STATUS_H))
            .constrain_to(screen)
            .frame(float_dock_frame(egui::vec2(0.0, 2.0)))
            .show(ctx, |ui| {
                self.ui_right_panel_body(ui, PanelChrome::Floating);
            });
        if let Some((rx, ry, rw, rh)) = floating_window_rect(ctx, egui::Id::new("right_mgr_float")) {
            let mut nx = rx;
            let mut ny = ry;
            let mut nw = rw;
            let mut nh = rh;
            layout::clamp_float_rect(&mut nx, &mut ny, &mut nw, &mut nh, screen);
            if (nx - self.right_float.x).abs() > 1.0
                || (ny - self.right_float.y).abs() > 1.0
                || (nw - self.right_float.w).abs() > 1.0
                || (nh - self.right_float.h).abs() > 1.0
            {
                self.right_float.x = nx;
                self.right_float.y = ny;
                self.right_float.w = nw;
                self.right_float.h = nh;
                persist = true;
            }
        }
        if !open {
            self.show_layer_mgr = false;
            persist = true;
        }
        if persist {
            self.persist_prefs();
        }
    }

    fn draw_left_autohide(&mut self, ctx: &egui::Context) {
        self.draw_left_rail(ctx, LeftRailKind::AutoHide);
        let now = Instant::now();
        if let Some(until) = self.left_autohide_until {
            if now < until {
                ctx.request_repaint_after(until.saturating_duration_since(now));
            } else if !self.left_autohide_open {
                self.left_autohide_until = None;
            }
        }
        let grace_alive = self
            .left_autohide_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false);
        if !(self.left_autohide_open || grace_alive) {
            return;
        }
        let screen = ctx.screen_rect();
        let top = layout::top_chrome_h();
        let height = layout::overlay_height(screen, chrome::STATUS_H_EXP + space::MD);
        let width = self.left_panel_width.clamp(220.0, 340.0);
        let mut still_open = false;
        egui::Area::new(egui::Id::new("left_mgr_autohide"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(panel_space::RAIL, top))
            .constrain_to(screen)
            .show(ctx, |ui| {
                float_dock_frame(egui::vec2(2.0, 0.0))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.set_height(height);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.ui_left_panel_body(ui, PanelChrome::AutoHide);
                            });
                    });
                if ui.rect_contains_pointer(ui.max_rect()) {
                    still_open = true;
                }
            });
        if still_open {
            self.left_autohide_open = true;
            self.left_autohide_until = Some(Instant::now() + AUTOHIDE_GRACE);
            ctx.request_repaint();
        } else if let Some(until) = self.left_autohide_until {
            if Instant::now() >= until {
                self.left_autohide_open = false;
                self.left_autohide_until = None;
            } else {
                ctx.request_repaint_after(until.saturating_duration_since(Instant::now()));
            }
        } else {
            self.left_autohide_open = false;
        }
    }

    fn draw_right_autohide(&mut self, ctx: &egui::Context) {
        self.draw_right_rail(ctx, RightRailKind::AutoHide);
        let now = Instant::now();
        if let Some(until) = self.right_autohide_until {
            if now < until {
                ctx.request_repaint_after(until.saturating_duration_since(now));
            } else if !self.right_autohide_open {
                self.right_autohide_until = None;
            }
        }
        let grace_alive = self
            .right_autohide_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false);
        if !(self.right_autohide_open || grace_alive) {
            return;
        }
        let screen = ctx.screen_rect();
        let top = layout::top_chrome_h();
        let height = layout::overlay_height(screen, chrome::STATUS_H_EXP + space::MD);
        let width = self.right_panel_width.clamp(220.0, 340.0);
        let x = (screen.width() - panel_space::RAIL - width).max(0.0);
        let mut still_open = false;
        egui::Area::new(egui::Id::new("right_mgr_autohide"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, top))
            .constrain_to(screen)
            .show(ctx, |ui| {
                float_dock_frame(egui::vec2(-2.0, 0.0))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.set_height(height);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.ui_right_panel_body(ui, PanelChrome::AutoHide);
                            });
                    });
                if ui.rect_contains_pointer(ui.max_rect()) {
                    still_open = true;
                }
            });
        if still_open {
            self.right_autohide_open = true;
            self.right_autohide_until = Some(Instant::now() + AUTOHIDE_GRACE);
            ctx.request_repaint();
        } else if let Some(until) = self.right_autohide_until {
            if Instant::now() >= until {
                self.right_autohide_open = false;
                self.right_autohide_until = None;
            } else {
                ctx.request_repaint_after(until.saturating_duration_since(Instant::now()));
            }
        } else {
            self.right_autohide_open = false;
        }
    }

    pub(super) fn ui_templates_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let panel_w = lock_panel_width(ui);
            section_label_first(ui, "Nameplate template");
            ui.label(
                egui::RichText::new(
                    "Draw on TMPL_BORDER / TMPL_VIEW / TMPL_TEXT, make a block, convert it, then INSERT. Text with {title} etc. fills from Drawing Properties.",
                )
                .small()
                .weak(),
            );
            ui.add_space(panel_space::TIGHT);
            if button_sized(ui, BtnKind::Primary, "Ensure template layers", panel_w).clicked() {
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
            ui.add_space(panel_space::ROW);
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
                if button(ui, BtnKind::Secondary, "Convert to template").clicked() {
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
                && button_sized(
                    ui,
                    BtnKind::Secondary,
                    &format!("INSERT {nameplate}"),
                    panel_w,
                )
                .clicked()
            {
                self.insert_block = nameplate.clone();
                self.tool = Tool::Insert {
                    name: nameplate.clone(),
                };
                self.note = format!("INSERT {nameplate} — click to place");
            }
            section_label(ui, "Property fields (place as TEXT)");
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
                    if link_button(ui, format!("{{{token}}}")).clicked() {
                        self.current_layer = "TMPL_TEXT".into();
                        self.dyn_input = format!("{{{token}}}");
                        self.tool = Tool::Text {
                            origin: None,
                            height: None,
                        };
                        self.note = format!(
                            "TEXT {{{token}}} — click point, then Space (layer TMPL_TEXT)"
                        );
                    }
                }
            });
            ui.add_space(panel_space::ROW);
            if ui.link("Open Drawing Properties").clicked() {
                self.open_drawing_properties();
            }
        });
    }

    pub(super) fn ui_hatch_manager(&mut self, ui: &mut egui::Ui) {
        let panel_w = lock_panel_width(ui);
        section_label_first(ui, "Fill kind");
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(panel_w);
            for (kind, label) in [
                (aadt_model::FillKind::Technical, "Technical"),
                (aadt_model::FillKind::Floor, "Floor"),
                (aadt_model::FillKind::Presentation, "Presentation"),
            ] {
                if ui
                    .selectable_label(self.fill_kind_tab == kind, label)
                    .clicked()
                {
                    self.fill_kind_tab = kind;
                }
            }
        });
        ui.add_space(4.0);
        section_label(ui, "Fill preview");
        ui.label(
            egui::RichText::new(format!("Active: {}", self.hatch_pattern))
                .small()
                .color(color::accent()),
        );
        if button_sized(ui, BtnKind::Primary, "Apply Fill (H)", panel_w).clicked() {
            self.tool = Tool::Hatch;
            self.note = format!(
                "FILL ({}) — click a closed polyline or circle",
                self.hatch_pattern
            );
        }
        // Edit: retarget the pattern of the selected hatch(es) to the active one.
        let sel = self.selected_hatches.len();
        if sel > 0 {
            let label = format!(
                "Set {sel} selected fill{} to {}",
                if sel == 1 { "" } else { "s" },
                self.hatch_pattern
            );
            if button_sized(ui, BtnKind::Secondary, &label, panel_w).clicked() {
                self.apply_pattern_to_selected_hatches();
            }
        }
        ui.add_space(panel_space::ROW);
        let root = aadt_library::default_library_root();
        let patterns = aadt_library::list_hatch_patterns(&root)
            .unwrap_or_else(|_| aadt_library::builtin_hatch_patterns());
        let patterns: Vec<_> = patterns
            .into_iter()
            .filter(|p| aadt_model::FillKind::from_pattern(&p.name) == self.fill_kind_tab)
            .collect();
        if patterns.is_empty() {
            ui.label(
                egui::RichText::new("No patterns in this fill kind yet.")
                    .small()
                    .weak(),
            );
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let gap = space::SM;
                let avail = lock_panel_width(ui);
                let tile = fit_tile_size(avail, panel_space::TILE_MIN, panel_space::TILE_MAX, gap);
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
            section_label_first(ui, "Insert");
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.label("Table rows");
                ui.add(egui::TextEdit::singleline(&mut self.table_rows).desired_width(40.0));
                ui.label("cols");
                ui.add(egui::TextEdit::singleline(&mut self.table_cols).desired_width(40.0));
            });
            if button_sized(ui, BtnKind::Primary, "Insert Table…", panel_w).clicked() {
                self.tool = Tool::TablePlace;
            }
            section_label(ui, "Leaders & marks");
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                if button(ui, BtnKind::Secondary, "Multileader (LE)").clicked() {
                    self.tool = Tool::Mleader {
                        arrow: None,
                        landing: None,
                        text_pt: None,
                    };
                }
                if button(ui, BtnKind::Secondary, "Callout / bubble").clicked() {
                    self.tool = Tool::Callout {
                        arrow: None,
                        center: None,
                    };
                }
                if button(ui, BtnKind::Secondary, "Section line").clicked() {
                    self.tool = Tool::SectionLine { first: None };
                }
                if button(ui, BtnKind::Secondary, "Revision mark").clicked() {
                    self.tool = Tool::RevMark;
                }
                if button(ui, BtnKind::Secondary, "Revision cloud").clicked() {
                    self.tool = Tool::RevCloud { first: None };
                }
            });
            section_label(ui, "Dimensions");
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                if button(ui, BtnKind::Secondary, "Aligned (DIM)").clicked() {
                    self.tool = Tool::Dim {
                        first: None,
                        second: None,
                    };
                    self.note = "DIM — click first point".into();
                }
                if button(ui, BtnKind::Secondary, "Continue (DIMCONTINUE)").clicked() {
                    self.tool = Tool::DimContinue;
                    self.note = "DIMCONTINUE — next definition point (Space finishes)".into();
                }
                if button(ui, BtnKind::Secondary, "Quick Dim (QDIM)").clicked() {
                    if self.selection.is_empty() {
                        self.note = "QDIM — select objects first, then pick dim-line side".into();
                    }
                    self.tool = Tool::QDim;
                    if !self.selection.is_empty() {
                        self.note = "QDIM — click dim-line side".into();
                    }
                }
                if button(ui, BtnKind::Secondary, "Auto Dim").clicked() {
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
                if button(ui, BtnKind::Secondary, "Radius").clicked() {
                    self.tool = Tool::DimRadius;
                    self.note = "DIMRADIUS — click a circle or arc".into();
                }
                if button(ui, BtnKind::Secondary, "Diameter").clicked() {
                    self.tool = Tool::DimDiameter;
                    self.note = "DIMDIAMETER — click a circle or arc".into();
                }
                if button(ui, BtnKind::Secondary, "Angular").clicked() {
                    self.tool = Tool::DimAngular {
                        vertex: None,
                        p1: None,
                    };
                    self.note = "DIMANGULAR — click vertex".into();
                }
            });
            if link_button(ui, "Restore layers (after Auto Dim)").clicked() {
                self.app_feed("AUTODIMRESTORE");
                if let Some(fb) = self.session.transcript().last() {
                    if let Some(msg) = fb.message() {
                        self.note = msg.to_string();
                    }
                }
            }
            if link_button(ui, "Open Dimension manager…").clicked() {
                self.right_tab = 5; // Dims
                self.reveal_right_mgr();
            }
            section_label(ui, "Object count");
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                if button(ui, BtnKind::Secondary, "Count all objects").clicked() {
                    self.app_feed("COUNT ALL");
                    if let Some(fb) = self.session.transcript().last() {
                        if let Some(msg) = fb.message() {
                            self.note = msg.to_string();
                        }
                    }
                }
                if button(ui, BtnKind::Secondary, "Count selection types").clicked() {
                    self.sync_selection();
                    self.app_feed("COUNT ALL");
                }
            });
            section_label(ui, "Auto-number");
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
            if button_sized(ui, BtnKind::Primary, "Number selection", panel_w).clicked() {
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
            section_label_first(ui, "Summary");
            prop_row(ui, "Title", &mut self.props_edit.title);
            prop_row(ui, "Subject", &mut self.props_edit.subject);
            prop_row(ui, "Author", &mut self.props_edit.author);
            prop_row(ui, "Keywords", &mut self.props_edit.keywords);
            ui.label("Comments");
            ui.add(
                egui::TextEdit::multiline(&mut self.props_edit.comments)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3),
            );
            prop_row(ui, "Hyperlink base", &mut self.props_edit.hyperlink_base);
            section_label(ui, "Title block (fixed layout / per-sheet attributes)");
            ui.label(
                egui::RichText::new(
                    "These fields also fill {title}, {project}, … text in nameplate templates (Templates tab).",
                )
                .small()
                .weak(),
            );
            prop_row(ui, "Company name", &mut self.props_edit.firm_name);
            prop_row(ui, "Tagline", &mut self.props_edit.firm_tagline);
            prop_row(ui, "Office address", &mut self.props_edit.office_address);
            prop_row(ui, "Phone", &mut self.props_edit.phone);
            prop_row(ui, "Email", &mut self.props_edit.email);
            prop_row(ui, "Website", &mut self.props_edit.website);
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
                if button(ui, BtnKind::Secondary, "Upload…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Logo", &["png", "jpg", "jpeg", "svg"])
                        .pick_file()
                    {
                        self.props_edit.logo_path = path.display().to_string();
                    }
                }
                if link_button(ui, "Clear").clicked() {
                    self.props_edit.logo_path.clear();
                }
            });
            section_label(ui, "Project (shared across set)");
            prop_row(ui, "Project name", &mut self.props_edit.project);
            prop_row(ui, "Client", &mut self.props_edit.client);
            prop_row(ui, "Address", &mut self.props_edit.address);
            prop_row(ui, "Project / job no.", &mut self.props_edit.project_number);
            prop_row(ui, "Architect", &mut self.props_edit.architect);
            prop_row(ui, "Area", &mut self.props_edit.area);
            section_label(ui, "This sheet (attributes)");
            prop_row(ui, "Drawing title", &mut self.props_edit.title);
            prop_row(ui, "Drawing number (A-101)", &mut self.props_edit.drawing_number);
            prop_row(ui, "Sheet number (01 of 45)", &mut self.props_edit.sheet_number);
            prop_row(ui, "Total sheets", &mut self.props_edit.total_sheets);
            prop_row(ui, "Discipline", &mut self.props_edit.discipline);
            prop_row(ui, "Scale", &mut self.props_edit.scale);
            prop_row(ui, "Date", &mut self.props_edit.date);
            prop_row(ui, "Drawn by", &mut self.props_edit.drawn_by);
            prop_row(ui, "Checked by", &mut self.props_edit.checked_by);
            prop_row(ui, "Approved by", &mut self.props_edit.approved_by);
            prop_row(ui, "Revision", &mut self.props_edit.revision);
            prop_row(ui, "Drawing status", &mut self.props_edit.drawing_status);
            ui.label("Revision history (Rev|Date|Description|By|Checked per line)");
            ui.add(
                egui::TextEdit::multiline(&mut self.props_edit.revision_history)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .hint_text("00|22-Jul-2026|Initial Issue|VH|RK"),
            );
            section_label(ui, "Building heights (general notes)");
            ui.label(
                egui::RichText::new(
                    "Declared heights are inserted into suggested general notes and can use {floor_height} / {lintel_height} in nameplate text.",
                )
                .small()
                .weak(),
            );
            prop_row(ui, "Floor-to-floor height", &mut self.props_edit.floor_height);
            prop_row(ui, "Lintel height (above FFL)", &mut self.props_edit.lintel_height);
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
                if button(ui, BtnKind::Secondary, "Reload suggested notes")
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
            section_label(ui, "Floors & model spaces");
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
                if button(ui, BtnKind::Secondary, "Add floor").clicked() {
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
                if button(ui, BtnKind::Secondary, "Add space").clicked() {
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
            section_label(ui, "Plot / nameplate");
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
            ui.add_space(panel_space::ROW);
            ui.horizontal(|ui| {
                if button(ui, BtnKind::Primary, "Apply").clicked() {
                    self.session
                        .set_drawing_properties(self.props_edit.clone());
                    self.dirty = true;
                    self.note = "Drawing properties applied".into();
                }
                if button(ui, BtnKind::Secondary, "Reload").clicked() {
                    self.props_edit = self.session.document().properties().clone();
                }
                if button(ui, BtnKind::Secondary, "Plot PDF…").clicked() {
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
            section_label_first(ui, "Primary units");
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

            section_label(ui, "Entry tips");
            match primary {
                aadt_model::UnitSystem::Meters => {
                    ui.label("• `3.5` → 3.5 metres");
                    ui.label("• `12'-6\"` still parses (converted to metres)");
                    ui.label(
                        "• Door/window wall cut recognises faces at 100 / 115 / 150 / 200 / 230 mm",
                    );
                }
                aadt_model::UnitSystem::FeetInches => {
                    ui.label("• `6` or `6.5` → inches (default)");
                    ui.label("• `6\"` → 6 inches");
                    ui.label("• `12'` → 12 feet");
                    ui.label("• `12'-6\"` → 12 ft 6 in");
                    ui.label("• Door/window wall cut recognises faces at 4½″ and 9″");
                }
            }

            section_label(ui, "Secondary unit display");
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

            ui.add_space(panel_space::ROW);
            ui.label(format!(
                "Dim style (from units): {}",
                if self.props_edit.dimstyle.is_empty() {
                    primary.dimstyle_name()
                } else {
                    &self.props_edit.dimstyle
                }
            ));
            if link_button(ui, "Open Dimension manager…").clicked() {
                self.right_tab = 5; // Dims
                self.reveal_right_mgr();
            }

            ui.add_space(panel_space::ROW);
            let half = half_button_width(panel_w);
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing.x = space::SM;
                if button_sized(ui, BtnKind::Primary, "Apply", half).clicked() {
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
                if button_sized(ui, BtnKind::Secondary, "Reload", half).clicked() {
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
            section_label_first(ui, "Place");
            ui.horizontal_wrapped(|ui| {
                ui.set_max_width(panel_w);
                ui.spacing_mut().item_spacing = egui::vec2(space::SM, space::SM);
                if button(ui, BtnKind::Secondary, "Aligned (DIM)").clicked() {
                    self.tool = Tool::Dim {
                        first: None,
                        second: None,
                    };
                    self.note = "DIM — click first point".into();
                }
                if button(ui, BtnKind::Secondary, "Continue (DIMCONTINUE)").clicked() {
                    self.tool = Tool::DimContinue;
                    self.note = "DIMCONTINUE — next definition point (Space finishes)".into();
                }
                if button(ui, BtnKind::Secondary, "Quick Dim (QDIM)").clicked() {
                    if self.selection.is_empty() {
                        self.note = "QDIM — select objects first, then pick dim-line side".into();
                    }
                    self.tool = Tool::QDim;
                    if !self.selection.is_empty() {
                        self.note = "QDIM — click dim-line side".into();
                    }
                }
                if button(ui, BtnKind::Secondary, "Auto Dim").clicked() {
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
                if button(ui, BtnKind::Secondary, "Radius").clicked() {
                    self.tool = Tool::DimRadius;
                    self.note = "DIMRADIUS — click a circle or arc".into();
                }
                if button(ui, BtnKind::Secondary, "Diameter").clicked() {
                    self.tool = Tool::DimDiameter;
                    self.note = "DIMDIAMETER — click a circle or arc".into();
                }
                if button(ui, BtnKind::Secondary, "Angular").clicked() {
                    self.tool = Tool::DimAngular {
                        vertex: None,
                        p1: None,
                    };
                    self.note = "DIMANGULAR — click vertex".into();
                }
            });
            if link_button(ui, "Restore layers (AUTODIMRESTORE)").clicked() {
                self.app_feed("AUTODIMRESTORE");
                if let Some(fb) = self.session.transcript().last() {
                    if let Some(msg) = fb.message() {
                        self.note = msg.to_string();
                    }
                }
            }

            section_label(ui, "Dimension style");
            prop_row(ui, "Style name", &mut self.props_edit.dimstyle);
            prop_row(ui, "Text height", &mut self.props_edit.dim_text_height);
            prop_row(ui, "Overall scale", &mut self.props_edit.dim_scale);

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

            ui.add_space(panel_space::ROW);
            if link_button(ui, "Open Units manager…").clicked() {
                self.right_tab = 4; // Units
                self.reveal_right_mgr();
            }

            ui.horizontal(|ui| {
                if button(ui, BtnKind::Primary, "Apply style").clicked() {
                    let sys = aadt_model::primary_from_props(&self.props_edit.units);
                    if self.props_edit.dimstyle.is_empty() {
                        self.props_edit.dimstyle = sys.dimstyle_name().to_string();
                    }
                    self.session.set_drawing_properties(self.props_edit.clone());
                    self.dirty = true;
                    self.note = "Dimension style applied".into();
                }
                if button(ui, BtnKind::Secondary, "Reload").clicked() {
                    self.props_edit = self.session.document().properties().clone();
                }
            });
        });
    }

    pub(super) fn draw_plot_dialog(&mut self, ctx: &egui::Context) {
        if !self.plot_open {
            return;
        }
        let (frame, screen, max_h) = dialog_defaults(ctx);
        let mut open = true;
        egui::Window::new("Plot to PDF")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .max_height(max_h)
            .constrain_to(screen)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(frame)
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
                        if button(ui, BtnKind::Secondary, "Pick window…").clicked() {
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
                                .color(color::text_secondary()),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("no window yet")
                                    .small()
                                    .color(color::text_secondary()),
                            );
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("View");
                    if ui
                        .selectable_label(
                            self.view_preset == super::ViewPreset::TechnicalPlan,
                            "Technical",
                        )
                        .on_hover_text("Apply Technical Plan layers before plot (hidden layers omitted)")
                        .clicked()
                    {
                        self.apply_view_preset(super::ViewPreset::TechnicalPlan);
                    }
                    if ui
                        .selectable_label(
                            self.view_preset == super::ViewPreset::Presentation,
                            "Presentation",
                        )
                        .on_hover_text(
                            "Apply Presentation board layers before plot (hidden layers omitted)",
                        )
                        .clicked()
                    {
                        self.apply_view_preset(super::ViewPreset::Presentation);
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "Plot uses current layer visibility — switch View above or status Options.",
                    )
                    .small()
                    .weak(),
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
                    if button(ui, BtnKind::Secondary, "Reload suggested").clicked() {
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
                    if button(ui, BtnKind::Subtle, "Cancel").clicked() {
                        self.plot_open = false;
                    }
                    if button(ui, BtnKind::Primary, "Export PDF…")
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
        let (frame, screen, max_h) = dialog_defaults(ctx);
        egui::Window::new("Map layers → spaces")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .max_height(max_h)
            .constrain_to(screen)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(frame)
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
                    if button(ui, BtnKind::Secondary, "Auto (heuristic)").clicked() {
                        for (layer, cat) in self.layer_map_draft.iter_mut() {
                            *cat = aadt_model::SpaceCategory::from_layer_name(layer);
                        }
                    }
                    if button(ui, BtnKind::Secondary, "All → Walls").clicked() {
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
                    if button(ui, BtnKind::Primary, "Apply map").clicked() {
                        self.apply_layer_map_draft();
                    }
                    if button(ui, BtnKind::Subtle, "Skip").clicked() {
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

fn floating_window_rect(ctx: &egui::Context, id: egui::Id) -> Option<(f32, f32, f32, f32)> {
    ctx.memory(|m| {
        m.area_rect(id)
            .map(|r| (r.min.x, r.min.y, r.width(), r.height()))
    })
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
    // Cap the content width (never *force* it via set_width/min — that lets a
    // wide row of tiles expand the whole SidePanel past its max_width, leaving a
    // large uncovered gap beside the dock). set_max_width lets content wrap.
    let w = ui.available_width().clamp(1.0, 340.0);
    ui.set_max_width(w);
    w
}

/// Tile edge so a grid of equal cells fills `available` (Fluent spacing gap).
fn fit_tile_size(available: f32, min_tile: f32, max_tile: f32, gap: f32) -> f32 {
    let avail = available.max(1.0);
    let min_t = min_tile.min(avail).max(36.0_f32.min(avail));
    let cols = ((avail + gap) / (min_t + gap)).floor().clamp(1.0, 5.0);
    let tile = (avail - gap * (cols - 1.0)) / cols;
    tile.clamp(36.0_f32.min(avail), max_tile)
}

fn caption_chars(tile: f32) -> usize {
    ((tile / 7.0).floor() as usize).clamp(4, 14)
}

fn half_button_width(panel_w: f32) -> f32 {
    ((panel_w - space::SM) * 0.5).max(72.0)
}

fn hatch_preview_tile(ui: &mut egui::Ui, name: &str, selected: bool, tile: f32) -> egui::Response {
    ui.vertical(|ui| {
        ui.set_width(tile);
        let (rect, resp) = tile_frame(ui, egui::vec2(tile, tile), selected);
        let inset = (tile * 0.09).clamp(3.0, 8.0);
        paint_hatch_swatch(ui.painter(), rect.shrink(inset), name);
        ui.label(
            egui::RichText::new(truncate_ui(name, caption_chars(tile)))
                .small()
                .color(if selected {
                    color::accent()
                } else {
                    color::text_secondary()
                }),
        );
        resp
    })
    .inner
}

fn paint_hatch_swatch(painter: &egui::Painter, r: egui::Rect, name: &str) {
    // Pattern ink contrasts with the (theme-filled) tile: dark on light paper,
    // light on the dark tile so the diagonals/bricks/dots stay legible.
    let ink = if color::is_dark() {
        egui::Color32::from_rgb(205, 205, 205)
    } else {
        egui::Color32::from_rgb(40, 40, 40)
    };
    let stroke = egui::Stroke::new(1.0_f32, ink);
    let u = name.to_ascii_uppercase();
    painter.rect_stroke(r, 0.0, egui::Stroke::new(0.5_f32, color::stroke()));
    // Clip every pattern to the swatch box: several families (diagonals, tri/net,
    // zigzag) intentionally over-draw the rect and rely on this clip to stay
    // inside their tile instead of bleeding across the panel.
    let painter = painter.with_clip_rect(r);

    if u == "SOLID" {
        let fill = if color::is_dark() {
            egui::Color32::from_rgb(150, 150, 150)
        } else {
            egui::Color32::from_rgb(90, 90, 90)
        };
        painter.rect_filled(r, 0.0, fill);
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
        let (rect, resp) = tile_frame(ui, egui::vec2(tile, tile), selected);
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
                    color::accent()
                } else {
                    color::text_secondary()
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
        let (rect, resp) = tile_frame(ui, egui::vec2(tile, tile), selected);
        let inset = (tile * 0.12).clamp(5.0, 12.0);
        paint_library_glyph(ui.painter(), rect.shrink(inset), path);
        ui.label(
            egui::RichText::new(truncate_ui(name, caption_chars(tile)))
                .small()
                .color(if selected {
                    color::accent()
                } else {
                    color::text_secondary()
                }),
        );
        resp
    })
    .inner
}

fn paint_placeholder_block(painter: &egui::Painter, r: egui::Rect) {
    let stroke = egui::Stroke::new(1.2_f32, color::text_secondary());
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
    let stroke = egui::Stroke::new(1.3_f32, color::text());
    if s.contains("door") {
        let leaf = egui::Rect::from_min_max(
            egui::pos2(r.center().x - 4.0, r.min.y + 4.0),
            egui::pos2(r.max.x - 4.0, r.max.y - 4.0),
        );
        painter.rect_stroke(leaf, 0.0, stroke);
        painter.circle_filled(
            egui::pos2(leaf.min.x + 5.0, leaf.center().y),
            1.5,
            color::text(),
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
            Geometry::Image(img) => pts.extend(img.corners()),
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
    let stroke = egui::Stroke::new(1.2_f32, color::text());
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
                painter.circle_filled(to_screen(t.position), 1.5, color::text_secondary());
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
            Geometry::Image(img) => {
                let corners = img.corners();
                let screen: Vec<_> = corners.iter().map(|p| to_screen(*p)).collect();
                for i in 0..4 {
                    painter.line_segment([screen[i], screen[(i + 1) % 4]], stroke);
                }
            }
        }
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
