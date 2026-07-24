//! Windows 11 / Fluent design tokens for AADT chrome — light, flat, clear depth.
//!
//! Guided by Microsoft design principles
//! ([Effortless, Calm, Personal, Familiar, Complete + Coherent](https://learn.microsoft.com/en-us/windows/apps/design/design-principles))
//! and app guidelines for [layout](https://learn.microsoft.com/en-us/windows/apps/design/layout/),
//! [navigation](https://learn.microsoft.com/en-us/windows/apps/design/basics/navigation-basics),
//! and [foundations](https://learn.microsoft.com/en-us/windows/apps/design/guidelines-overview).

use eframe::egui;

/// Fluent-ish spacing scale (4 px base).
pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    #[allow(dead_code)]
    pub const LG: f32 = 16.0;
}

/// Corner radii — flat chrome (no rounded controls).
pub mod radius {
    pub const CONTROL: f32 = 0.0;
    #[allow(dead_code)]
    pub const CARD: f32 = 0.0;
}

/// Light theme palette: calm foundation; accent only for hierarchy / selection.
/// Depth comes from fill steps + 1px strokes, not shadows or rounding.
pub mod color {
    use eframe::egui::Color32;

    /// App chrome / panel fill (Layer 1 — recessed).
    pub const LAYER_BASE: Color32 = Color32::from_rgb(243, 243, 243);
    /// Elevated surfaces (side panel, dialogs).
    pub const LAYER_CARD: Color32 = Color32::from_rgb(252, 252, 252);
    /// Drawing paper (content focus).
    pub const CANVAS: Color32 = Color32::from_rgb(255, 255, 255);
    /// Subtle control rest fill.
    pub const CONTROL: Color32 = Color32::from_rgb(243, 243, 243);
    pub const CONTROL_HOVER: Color32 = Color32::from_rgb(229, 229, 229);
    pub const CONTROL_PRESSED: Color32 = Color32::from_rgb(210, 210, 210);
    /// Selected / accent (Windows blue).
    pub const ACCENT: Color32 = Color32::from_rgb(0, 95, 184);
    /// Soft accent fill (≈11% opacity blue over white).
    pub const ACCENT_SOFT: Color32 = Color32::from_rgb(230, 240, 250);
    pub const TEXT: Color32 = Color32::from_rgb(26, 26, 26);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(92, 92, 92);
    pub const STROKE: Color32 = Color32::from_rgb(218, 218, 218);
    pub const DIVIDER: Color32 = Color32::from_rgb(200, 200, 200);
    /// Window / panel outer edge (slightly stronger than internal strokes).
    pub const BORDER: Color32 = Color32::from_rgb(190, 190, 190);
}

/// Apply calm Fluent-aligned egui visuals (light, flat).
pub fn apply_fluent_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.dark_mode = false;
    visuals.panel_fill = color::LAYER_BASE;
    visuals.window_fill = color::LAYER_CARD;
    visuals.extreme_bg_color = color::CONTROL;
    visuals.faint_bg_color = color::CONTROL;
    visuals.override_text_color = Some(color::TEXT);
    visuals.window_stroke = egui::Stroke::new(1.0_f32, color::BORDER);
    visuals.widgets.noninteractive.bg_fill = color::LAYER_BASE;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, color::STROKE);
    visuals.widgets.inactive.bg_fill = color::CONTROL;
    visuals.widgets.inactive.weak_bg_fill = color::CONTROL;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, color::STROKE);
    visuals.widgets.hovered.bg_fill = color::CONTROL_HOVER;
    visuals.widgets.hovered.weak_bg_fill = color::CONTROL_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, color::DIVIDER);
    visuals.widgets.active.bg_fill = color::CONTROL_PRESSED;
    visuals.widgets.active.weak_bg_fill = color::ACCENT_SOFT;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, color::ACCENT);
    visuals.widgets.open.bg_fill = color::LAYER_CARD;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, color::BORDER);
    visuals.selection.bg_fill = color::ACCENT_SOFT;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, color::ACCENT);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, color::TEXT);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, color::TEXT);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, color::TEXT);
    // Flat: zero corner radius everywhere.
    let flat = egui::Rounding::ZERO;
    visuals.window_rounding = flat;
    visuals.menu_rounding = flat;
    visuals.widgets.noninteractive.rounding = flat;
    visuals.widgets.inactive.rounding = flat;
    visuals.widgets.hovered.rounding = flat;
    visuals.widgets.active.rounding = flat;
    visuals.widgets.open.rounding = flat;
    ctx.set_visuals(visuals);

    // Prefer OS light chrome (title bar / border) to match in-app light theme.
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Light));

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(space::SM, space::XS);
    style.spacing.button_padding = egui::vec2(space::SM, space::XS);
    style.spacing.window_margin = egui::Margin::same(space::MD);
    style.spacing.interact_size.y = 28.0;
    ctx.set_style(style);
}

/// Peer destinations in the right manager (flat / lateral nav).
pub struct NavDest {
    pub tab: usize,
    pub label: &'static str,
    pub tip: &'static str,
    pub icon: super::icons::Icon,
}

/// Flat peer group for the side manager — clearly labeled destinations
/// ([navigation basics](https://learn.microsoft.com/en-us/windows/apps/design/basics/navigation-basics)).
pub fn manager_nav() -> &'static [NavDest] {
    use super::icons::Icon;
    &[
        NavDest {
            tab: 0,
            label: "Layers",
            tip: "Layer properties",
            icon: Icon::Layers,
        },
        NavDest {
            tab: 1,
            label: "Blocks",
            tip: "Block definitions",
            icon: Icon::Blocks,
        },
        NavDest {
            tab: 2,
            label: "Takeoff",
            tip: "Quantities",
            icon: Icon::Select,
        },
        NavDest {
            tab: 3,
            label: "Hatches",
            tip: "Hatch patterns",
            icon: Icon::Hatch,
        },
        NavDest {
            tab: 4,
            label: "Annotate",
            tip: "Annotation tools",
            icon: Icon::Annotate,
        },
        NavDest {
            tab: 5,
            label: "Templates",
            tip: "Nameplate template layers and fields",
            icon: Icon::Blocks,
        },
        NavDest {
            tab: 6,
            label: "Units",
            tip: "Drawing units manager",
            icon: Icon::Props,
        },
        NavDest {
            tab: 7,
            label: "Dims",
            tip: "Dimension manager",
            icon: Icon::Annotate,
        },
        NavDest {
            tab: 8,
            label: "History",
            tip: "Drawing time machine / versions",
            icon: Icon::Props,
        },
    ]
}

pub fn manager_title(tab: usize) -> &'static str {
    match tab {
        1 => "Blocks",
        2 => "Takeoff",
        3 => "Hatches",
        4 => "Annotate",
        5 => "Nameplate templates",
        6 => "Units",
        7 => "Dimensions",
        8 => "History",
        _ => "Layers",
    }
}

/// Horizontal peer nav with accent indicator (selected = clear path).
pub fn nav_tabs(ui: &mut egui::Ui, current: &mut usize) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = space::XS;
        for dest in manager_nav() {
            let selected = *current == dest.tab;
            let text = if selected {
                egui::RichText::new(dest.label).strong().color(color::ACCENT)
            } else {
                egui::RichText::new(dest.label).color(color::TEXT_SECONDARY)
            };
            let resp = ui
                .add(egui::SelectableLabel::new(selected, text))
                .on_hover_text(dest.tip);
            if selected {
                let r = resp.rect;
                ui.painter().line_segment(
                    [
                        egui::pos2(r.min.x, r.max.y - 1.0),
                        egui::pos2(r.max.x, r.max.y - 1.0),
                    ],
                    egui::Stroke::new(2.0_f32, color::ACCENT),
                );
            }
            if resp.clicked() {
                *current = dest.tab;
                changed = true;
            }
        }
    });
    changed
}
