//! The first line of the top bar: a row of tool buttons the user chooses and orders.
//!
//! The second line is the context bar (layer, floor, drawing space: [`crate::contextbar`]). The
//! first line is shown in Classic only (Pro has no toolbars) and every button just starts the
//! command it names, like the menu item or the command line does. What is on it is
//! `UiState::top_bar`, a list of command ids (and `-` for a separator): right-click the bar, or use
//! View > Customize Top Bar, to change it. Each workspace mode keeps its own list.

use serde_json::{Value, json};

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::palettes::{DRAFTING, command_known};
use crate::theme::Tokens;

const BUTTON: f32 = 26.0;
/// The separator entry.
pub const SEPARATOR: &str = "-";
/// Most entries on the bar.
pub const MAX_ITEMS: usize = 40;

/// One tool that can be put on the bar.
#[derive(Clone, Debug)]
pub struct Tool {
    pub group: &'static str,
    pub icon: Icon,
    pub cmd: &'static str,
    pub label: &'static str,
}

/// Everything that can be put on the bar: the drafting tool groups and the architecture tools, each
/// once, in the order the toolbars list them.
pub fn catalog() -> Vec<Tool> {
    let mut out: Vec<Tool> = Vec::new();
    for (group, large, small) in DRAFTING {
        for (icon, cmd, label) in large.iter().chain(small.iter()) {
            if command_known(cmd) && !out.iter().any(|t| t.cmd == *cmd) {
                out.push(Tool { group, icon: *icon, cmd, label });
            }
        }
    }
    for (icon, cmd, label) in crate::toolbars::ARCHITECTURE {
        if command_known(cmd) && !out.iter().any(|t| t.cmd == *cmd) {
            out.push(Tool { group: "Architecture", icon: *icon, cmd, label });
        }
    }
    out
}

/// What the bar holds until the user changes it: the architecture tools.
pub fn default_items() -> Vec<String> {
    crate::toolbars::ARCHITECTURE.iter().map(|(_, cmd, _)| (*cmd).to_string()).collect()
}

/// Keep only entries that are separators or tools in the catalog (no repeats of a tool, no leading,
/// trailing or doubled separators), at most [`MAX_ITEMS`]. Saved lists from older builds or edited
/// by hand come through here.
pub fn sanitize(items: &[String]) -> Vec<String> {
    let cat = catalog();
    let mut out: Vec<String> = Vec::new();
    for i in items {
        if out.len() >= MAX_ITEMS {
            break;
        }
        if i == SEPARATOR {
            if !out.is_empty() && out.last().map(String::as_str) != Some(SEPARATOR) {
                out.push(i.clone());
            }
        } else if cat.iter().any(|t| t.cmd == i) && !out.contains(i) {
            out.push(i.clone());
        }
    }
    while out.last().map(String::as_str) == Some(SEPARATOR) {
        out.pop();
    }
    out
}

/// The first line.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let cat = catalog();
    let items = app.ui.top_bar.clone();
    egui::Panel::top("aad_tb_row").exact_size(34.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let resp = ui
            .horizontal_centered(|ui| {
                ui.add_space(8.0);
                if items.is_empty() {
                    ui.label(egui::RichText::new(crate::tl!("Right-click here to choose the tools on this bar")).small().weak());
                }
                ui.spacing_mut().item_spacing.x = 3.0;
                for id in &items {
                    if id == SEPARATOR {
                        ui.separator();
                    } else if let Some(tool) = cat.iter().find(|t| t.cmd == id) {
                        let r = icons::button(ui, tool.icon, BUTTON, tool.label, false);
                        if r.clicked() {
                            app.start(tool.cmd);
                        }
                    }
                }
                ui.allocate_response(egui::vec2(ui.available_width().max(0.0), BUTTON), egui::Sense::click())
            })
            .inner;
        resp.context_menu(|ui| {
            if ui.button(crate::tl!("Customize Top Bar…")).clicked() {
                app.ui.dialog = Some("customizetop".into());
                ui.close();
            }
            if ui.button(crate::tl!("Reset Top Bar")).clicked() {
                app.ui.top_bar = default_items();
                ui.close();
            }
        });
    });
}

/// The Customize Top Bar dialog: every tool with a tick to put it on the bar, and the bar's own list
/// with move and remove buttons. Changes apply at once.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let cat = catalog();
    egui::Window::new(crate::tl!("Customize Top Bar"))
        .id(egui::Id::new("customize_top_bar"))
        .open(open)
        .default_size(egui::vec2(560.0, 420.0))
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(crate::tl!("The first line of the top bar. The second line always shows layer, floor and drawing space."))
                    .small()
                    .weak(),
            );
            ui.separator();
            ui.columns(2, |cols| {
                cols[0].label(egui::RichText::new(crate::tl!("Available tools")).strong());
                egui::ScrollArea::vertical().id_salt("topbar_catalog").max_height(300.0).show(&mut cols[0], |ui| {
                    let mut group = "";
                    for tool in &cat {
                        if tool.group != group {
                            group = tool.group;
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(crate::i18n::t(group)).small().weak());
                        }
                        let mut on = app.ui.top_bar.iter().any(|i| i == tool.cmd);
                        let before = on;
                        ui.checkbox(&mut on, crate::i18n::t(tool.label));
                        if on != before {
                            if on {
                                if app.ui.top_bar.len() < MAX_ITEMS {
                                    app.ui.top_bar.push(tool.cmd.to_string());
                                }
                            } else {
                                app.ui.top_bar.retain(|i| i != tool.cmd);
                            }
                            app.ui.top_bar = sanitize(&app.ui.top_bar);
                        }
                    }
                });
                cols[1].label(egui::RichText::new(crate::tl!("On the bar, left to right")).strong());
                let mut action: Option<(usize, i8)> = None; // (index, -1 up, 1 down, 0 remove)
                egui::ScrollArea::vertical().id_salt("topbar_list").max_height(300.0).show(&mut cols[1], |ui| {
                    if app.ui.top_bar.is_empty() {
                        ui.label(egui::RichText::new(crate::tl!("Nothing yet: tick tools on the left.")).weak());
                    }
                    for (i, id) in app.ui.top_bar.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let name = if id == SEPARATOR {
                                crate::tl!("— separator —").to_string()
                            } else {
                                cat.iter().find(|t| t.cmd == id).map_or(id.clone(), |t| crate::i18n::t(t.label).to_string())
                            };
                            ui.label(name);
                            if ui.small_button("▲").on_hover_text(crate::tl!("Move left")).clicked() {
                                action = Some((i, -1));
                            }
                            if ui.small_button("▼").on_hover_text(crate::tl!("Move right")).clicked() {
                                action = Some((i, 1));
                            }
                            if ui.small_button("✕").on_hover_text(crate::tl!("Remove")).clicked() {
                                action = Some((i, 0));
                            }
                        });
                    }
                });
                if let Some((i, dir)) = action {
                    let v = &mut app.ui.top_bar;
                    match dir {
                        0 => {
                            v.remove(i);
                        }
                        -1 if i > 0 => v.swap(i, i - 1),
                        1 if i + 1 < v.len() => v.swap(i, i + 1),
                        _ => {}
                    }
                    app.ui.top_bar = sanitize(&app.ui.top_bar);
                }
                cols[1].horizontal(|ui| {
                    if ui.button(crate::tl!("Add separator")).clicked() && app.ui.top_bar.len() < MAX_ITEMS {
                        app.ui.top_bar.push(SEPARATOR.to_string());
                        app.ui.top_bar = sanitize(&app.ui.top_bar);
                    }
                    if ui.button(crate::tl!("Reset")).clicked() {
                        app.ui.top_bar = default_items();
                    }
                });
            });
        });
}

/// `ui.topbar.set {items}` and `ui.topbar.get`: the bar's list for scripts and the control channel.
pub fn command(app: &mut CadApp, params: &Value, set: bool) -> Result<Value, String> {
    if set {
        let Some(list) = params.get("items").and_then(Value::as_array) else { return Err("`items` must be a list of command ids and \"-\"".into()) };
        let asked: Vec<String> = list.iter().filter_map(Value::as_str).map(str::to_string).collect();
        if asked.len() != list.len() {
            return Err("every item must be text".into());
        }
        let cat = catalog();
        if let Some(bad) = asked.iter().find(|i| *i != SEPARATOR && !cat.iter().any(|t| t.cmd == *i)) {
            return Err(format!("`{bad}` is not a tool that can be put on the bar"));
        }
        app.ui.top_bar = sanitize(&asked);
    }
    Ok(json!({"items": app.ui.top_bar}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_has_each_tool_once_and_only_known_commands() {
        let c = catalog();
        assert!(c.len() > 30);
        let mut seen = std::collections::HashSet::new();
        assert!(c.iter().all(|t| seen.insert(t.cmd) && command_known(t.cmd)));
        assert!(c.iter().any(|t| t.cmd == "line") && c.iter().any(|t| t.cmd == "room"));
    }

    #[test]
    fn sanitize_drops_unknown_repeats_and_stray_separators() {
        let l = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(sanitize(&l(&["-", "line", "-", "-", "circle", "line", "nonsense", "-"])), l(&["line", "-", "circle"]));
        assert!(sanitize(&l(&["-", "-"])).is_empty());
        assert_eq!(sanitize(&default_items()), default_items(), "the default is valid as it is");
        let many: Vec<String> = (0..200).map(|_| SEPARATOR.to_string()).chain(catalog().iter().map(|t| t.cmd.to_string())).collect();
        assert!(sanitize(&many).len() <= MAX_ITEMS);
    }
}
