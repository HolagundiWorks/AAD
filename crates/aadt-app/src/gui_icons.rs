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
    Ai,
    Plot,
    Add,
    Delete,
    Select,
    Dismiss,
    /// Collapse the right manager panel.
    CollapsePanel,
    /// Expand the right manager panel (thin strip).
    ExpandPanel,
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
            Icon::Ai => egui::include_image!("../assets/fluent/sparkle_24_regular.svg"),
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
        }
    }
}

/// Register SVG loaders so Fluent icons render.
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

/// Clickable Fluent icon button (28×28 chrome).
pub fn icon_btn(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    icon_btn_sized(ui, icon, tip, egui::vec2(28.0, 28.0), 16.0)
}

pub fn icon_btn_sized(
    ui: &mut egui::Ui,
    icon: Icon,
    tip: &str,
    size: egui::Vec2,
    glyph: f32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_text(tip);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let bg = if resp.is_pointer_button_down_on() {
            egui::Color32::from_rgb(170, 190, 220)
        } else if resp.hovered() {
            egui::Color32::from_rgb(200, 210, 230)
        } else {
            egui::Color32::from_rgb(230, 230, 230)
        };
        painter.rect_filled(rect, 3.0, bg);
        let img = egui::Image::new(icon.svg())
            .fit_to_exact_size(egui::vec2(glyph, glyph))
            .tint(egui::Color32::from_rgb(40, 40, 40));
        let img_size = egui::vec2(glyph, glyph);
        let img_rect = egui::Rect::from_center_size(rect.center(), img_size);
        img.paint_at(ui, img_rect);
    }
    resp
}
