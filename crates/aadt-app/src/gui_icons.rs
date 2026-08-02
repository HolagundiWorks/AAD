//! WinUI / Fluent System Icons (MIT) for the AADT chrome.
//!
//! SVGs are vendored from microsoft/fluentui-system-icons under MIT.

use eframe::egui;

/// Toolbar / manager icons used by the chrome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    Undo,
    Redo,
    Layers,
    Blocks,
    Props,
    Hatch,
    Annotate,
    Plot,
    Add,
    Delete,
    Select,
    Dismiss,
    /// Collapse the right manager panel.
    CollapsePanel,
    /// Expand the right manager panel (thin strip).
    ExpandPanel,
    /// Layer on (visible).
    Eye,
    /// Layer off.
    EyeOff,
    /// Layer frozen.
    Freeze,
    /// Layer locked.
    Lock,
    /// Layer unlocked.
    Unlock,
    /// Dimension manager.
    Dims,
    /// Drawing history / versions.
    History,
    /// Nameplate templates.
    Templates,
    /// Object / selection properties.
    ObjectProps,
}

impl Icon {
    fn svg(self) -> egui::ImageSource<'static> {
        match self {
            Icon::New => egui::include_image!("../assets/fluent/document_add_24_regular.svg"),
            Icon::Open => egui::include_image!("../assets/fluent/folder_open_24_regular.svg"),
            Icon::Save => egui::include_image!("../assets/fluent/save_24_regular.svg"),
            Icon::Undo => egui::include_image!("../assets/fluent/arrow_undo_24_regular.svg"),
            Icon::Redo => egui::include_image!("../assets/fluent/arrow_redo_24_regular.svg"),
            Icon::Layers => egui::include_image!("../assets/fluent/layer_24_regular.svg"),
            Icon::Blocks => egui::include_image!("../assets/fluent/cube_24_regular.svg"),
            Icon::Props => egui::include_image!("../assets/fluent/document_text_24_regular.svg"),
            Icon::Hatch => egui::include_image!("../assets/fluent/grid_24_regular.svg"),
            Icon::Annotate => egui::include_image!("../assets/fluent/text_font_24_regular.svg"),
            Icon::Plot => egui::include_image!("../assets/fluent/print_24_regular.svg"),
            Icon::Add => egui::include_image!("../assets/fluent/add_24_regular.svg"),
            Icon::Delete => egui::include_image!("../assets/fluent/delete_24_regular.svg"),
            Icon::Select => egui::include_image!("../assets/fluent/cursor_24_regular.svg"),
            Icon::Dismiss => egui::include_image!("../assets/fluent/dismiss_24_regular.svg"),
            Icon::CollapsePanel => {
                egui::include_image!("../assets/fluent/panel_right_contract_24_regular.svg")
            }
            Icon::ExpandPanel => {
                egui::include_image!("../assets/fluent/chevron_left_24_regular.svg")
            }
            Icon::Eye => egui::include_image!("../assets/fluent/eye_24_regular.svg"),
            Icon::EyeOff => egui::include_image!("../assets/fluent/eye_off_24_regular.svg"),
            Icon::Freeze => {
                egui::include_image!("../assets/fluent/weather_snowflake_24_regular.svg")
            }
            Icon::Lock => egui::include_image!("../assets/fluent/lock_closed_24_regular.svg"),
            Icon::Unlock => egui::include_image!("../assets/fluent/lock_open_24_regular.svg"),
            Icon::Dims => egui::include_image!("../assets/fluent/ruler_24_regular.svg"),
            Icon::History => egui::include_image!("../assets/fluent/history_24_regular.svg"),
            Icon::Templates => egui::include_image!("../assets/fluent/slide_text_24_regular.svg"),
            Icon::ObjectProps => egui::include_image!("../assets/fluent/options_24_regular.svg"),
        }
    }

    /// Filled variant for selected states (Fluent: "Filled … for highlighting
    /// selected states"). Vendored for the nav-rail set; other icons fall back
    /// to their regular glyph. <https://fluent2.microsoft.design/iconography>
    fn svg_filled(self) -> egui::ImageSource<'static> {
        match self {
            Icon::Layers => egui::include_image!("../assets/fluent/layer_24_filled.svg"),
            Icon::Blocks => egui::include_image!("../assets/fluent/cube_24_filled.svg"),
            Icon::Hatch => egui::include_image!("../assets/fluent/grid_24_filled.svg"),
            Icon::Annotate => egui::include_image!("../assets/fluent/text_font_24_filled.svg"),
            Icon::Templates => egui::include_image!("../assets/fluent/slide_text_24_filled.svg"),
            Icon::Props => egui::include_image!("../assets/fluent/document_text_24_filled.svg"),
            Icon::Dims => egui::include_image!("../assets/fluent/ruler_24_filled.svg"),
            Icon::History => egui::include_image!("../assets/fluent/history_24_filled.svg"),
            Icon::ObjectProps => egui::include_image!("../assets/fluent/options_24_filled.svg"),
            other => other.svg(),
        }
    }
}

/// Register SVG loaders so Fluent icons render.
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

/// Bold monoline brand mark (soft-apex open A on orange). See `docs/branding/`.
pub fn brand_mark() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/branding/aadt_mark.svg")
}

/// Paint the solid orange monogram at `size`.
pub fn brand_mark_widget(ui: &mut egui::Ui, size: f32) -> egui::Response {
    ui.add(
        egui::Image::new(brand_mark())
            .fit_to_exact_size(egui::vec2(size, size))
            .sense(egui::Sense::hover()),
    )
    .on_hover_text("AAD — Accelerated Architectural Drafting")
}

/// Clickable Fluent icon button — compact toolbar (Cursor-like). 16px glyph in
/// a 28px target (Fluent System Icons UI step + comfortable hit area).
pub fn icon_btn(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    icon_btn_sized(ui, icon, tip, egui::vec2(28.0, 28.0), 16.0)
}

/// Compact icon for dense lists (layer rows).
pub fn icon_btn_compact(ui: &mut egui::Ui, icon: Icon, tip: &str, active: bool) -> egui::Response {
    icon_btn_sized_tinted(
        ui,
        icon,
        tip,
        egui::vec2(20.0, 20.0),
        14.0,
        if active {
            super::theme::color::ACCENT
        } else {
            super::theme::color::TEXT_SECONDARY
        },
    )
}

pub fn icon_btn_sized(
    ui: &mut egui::Ui,
    icon: Icon,
    tip: &str,
    size: egui::Vec2,
    glyph: f32,
) -> egui::Response {
    icon_btn_sized_tinted(ui, icon, tip, size, glyph, super::theme::color::TEXT)
}

fn icon_btn_sized_tinted(
    ui: &mut egui::Ui,
    icon: Icon,
    tip: &str,
    size: egui::Vec2,
    glyph: f32,
    tint: egui::Color32,
) -> egui::Response {
    use super::theme::{color, radius};
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let bg = if resp.is_pointer_button_down_on() {
            color::CONTROL_PRESSED
        } else if resp.hovered() {
            color::CONTROL_HOVER
        } else {
            egui::Color32::TRANSPARENT
        };
        if bg.a() > 0 {
            painter.rect_filled(rect, radius::CONTROL, bg);
            painter.rect_stroke(
                rect,
                radius::CONTROL,
                egui::Stroke::new(1.0_f32, color::STROKE),
            );
        }
        let img = egui::Image::new(icon.svg())
            .fit_to_exact_size(egui::vec2(glyph, glyph))
            .tint(tint);
        let img_size = egui::vec2(glyph, glyph);
        let img_rect = egui::Rect::from_center_size(rect.center(), img_size);
        img.paint_at(ui, img_rect);
    }
    resp
}

/// Selected nav-rail icon (accent soft fill) — flat, compact.
pub fn icon_btn_nav(ui: &mut egui::Ui, icon: Icon, tip: &str, selected: bool) -> egui::Response {
    use super::theme::{color, radius};
    let size = egui::vec2(32.0, 32.0);
    let glyph = 16.0;
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let bg = if selected {
            color::ACCENT_SOFT
        } else if resp.is_pointer_button_down_on() {
            color::CONTROL_PRESSED
        } else if resp.hovered() {
            color::CONTROL_HOVER
        } else {
            egui::Color32::TRANSPARENT
        };
        if bg.a() > 0 || selected {
            painter.rect_filled(rect, radius::CONTROL, bg);
        }
        if selected {
            painter.rect_stroke(
                rect,
                radius::CONTROL,
                egui::Stroke::new(1.0_f32, color::ACCENT),
            );
        }
        let tint = if selected { color::ACCENT } else { color::TEXT };
        // Fluent: filled glyph for the selected state, regular otherwise.
        let src = if selected {
            icon.svg_filled()
        } else {
            icon.svg()
        };
        let img = egui::Image::new(src)
            .fit_to_exact_size(egui::vec2(glyph, glyph))
            .tint(tint);
        img.paint_at(
            ui,
            egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph)),
        );
    }
    resp
}
