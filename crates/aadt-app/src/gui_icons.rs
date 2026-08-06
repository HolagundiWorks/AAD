//! WinUI / Fluent System Icons (MIT) for the AADT chrome.
//!
//! SVGs are vendored from microsoft/fluentui-system-icons under MIT. Masters
//! are recolored to white fill so a runtime `.tint(color)` (multiply) resolves
//! to the exact requested colour in both light and dark themes. The `Moon` /
//! `Sun` theme-toggle glyphs under `assets/ui/` are original (not Fluent).

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
    /// Float a docked manager into a free window.
    FloatPanel,
    /// Re-dock a floating / auto-hide manager to the edge.
    DockPanel,
    /// Pin (keep docked / exit auto-hide).
    PinPanel,
    /// Unpin into auto-hide (expand on hover).
    UnpinPanel,
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
    /// Theme toggle — switch to dark (shown in light mode).
    Moon,
    /// Theme toggle — switch to light (shown in dark mode).
    Sun,
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
            Icon::FloatPanel => egui::include_image!("../assets/fluent/window_24_regular.svg"),
            Icon::DockPanel => {
                egui::include_image!("../assets/fluent/panel_left_24_regular.svg")
            }
            Icon::PinPanel => egui::include_image!("../assets/fluent/pin_24_regular.svg"),
            Icon::UnpinPanel => egui::include_image!("../assets/fluent/pin_off_24_regular.svg"),
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
            Icon::Moon => egui::include_image!("../assets/ui/weather_moon.svg"),
            Icon::Sun => egui::include_image!("../assets/ui/weather_sunny.svg"),
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

/// AAD brand mark — the square "AD" monogram on brand amber (source: `SQ.svg`
/// in the repo root). See `docs/branding/`.
pub fn brand_mark() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/branding/aadt_mark.svg")
}

/// Paint the square brand mark at `size`.
pub fn brand_mark_widget(ui: &mut egui::Ui, size: f32) -> egui::Response {
    ui.add(
        egui::Image::new(brand_mark())
            .fit_to_exact_size(egui::vec2(size, size))
            .sense(egui::Sense::hover()),
    )
    .on_hover_text("AAD — Accelerated Architectural Drafting")
}

/// Clickable Fluent icon button — compact toolbar. 16px glyph in a 28px target.
pub fn icon_btn(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    use super::theme::icon as sz;
    icon_btn_sized(
        ui,
        icon,
        tip,
        egui::vec2(sz::HIT_TOOLBAR, sz::HIT_TOOLBAR),
        sz::GLYPH_MD,
    )
}

/// Compact icon for dense lists (layer rows).
pub fn icon_btn_compact(ui: &mut egui::Ui, icon: Icon, tip: &str, active: bool) -> egui::Response {
    use super::theme::icon as sz;
    icon_btn_sized_tinted(
        ui,
        icon,
        tip,
        egui::vec2(sz::HIT_COMPACT, sz::HIT_COMPACT),
        sz::GLYPH_SM,
        if active {
            super::theme::color::text()
        } else {
            super::theme::color::text_secondary()
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
    icon_btn_sized_tinted(ui, icon, tip, size, glyph, super::theme::color::text())
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
            color::control_pressed()
        } else if resp.hovered() {
            color::control_hover()
        } else {
            egui::Color32::TRANSPARENT
        };
        if bg.a() > 0 {
            painter.rect_filled(rect, radius::CONTROL, bg);
            painter.rect_stroke(
                rect,
                radius::CONTROL,
                egui::Stroke::new(1.0_f32, color::stroke()),
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

/// Selected nav-rail icon — quiet fill + left accent bar (Chrome 2.0).
pub fn icon_btn_nav(ui: &mut egui::Ui, icon: Icon, tip: &str, selected: bool) -> egui::Response {
    use super::theme::{color, icon as sz, radius};
    let size = egui::vec2(sz::HIT_NAV, sz::HIT_NAV);
    let glyph = sz::GLYPH_MD;
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let bg = if selected {
            color::control_hover()
        } else if resp.is_pointer_button_down_on() {
            color::control_pressed()
        } else if resp.hovered() {
            color::control_hover()
        } else {
            egui::Color32::TRANSPARENT
        };
        if bg.a() > 0 || selected {
            painter.rect_filled(rect, radius::CONTROL, bg);
        }
        if selected {
            let bar = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + 3.0),
                egui::pos2(rect.left() + 2.0, rect.bottom() - 3.0),
            );
            painter.rect_filled(bar, 0.0, color::accent());
        }
        let tint = color::text();
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
