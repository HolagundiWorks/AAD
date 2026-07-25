//! Fluent 2 design language for AADT chrome.
//!
//! Grounded in [Fluent 2 design principles](https://fluent2.microsoft.design/design-principles)
//! (natural on every platform · built for focus · one for all · unmistakably Microsoft)
//! and foundations: [color](https://fluent2.microsoft.design/color),
//! [elevation](https://fluent2.microsoft.design/elevation),
//! [iconography](https://fluent2.microsoft.design/iconography),
//! [layout](https://fluent2.microsoft.design/layout),
//! [material](https://fluent2.microsoft.design/material),
//! [motion](https://fluent2.microsoft.design/motion),
//! [shapes](https://fluent2.microsoft.design/shapes),
//! [typography](https://fluent2.microsoft.design/typography).
//!
//! Brand orange (`docs/branding/README.md`) is applied sparingly — selection, focus, CTAs —
//! so neutrals carry hierarchy and the canvas stays the focus region.

use eframe::egui;

/// Fluent 4px spacing ramp (subset used in chrome).
/// <https://fluent2.microsoft.design/layout>
#[allow(dead_code)]
pub mod space {
    /// size20
    pub const XS: f32 = 2.0;
    /// size40
    pub const SM: f32 = 4.0;
    /// size80
    pub const MD: f32 = 8.0;
    /// size120
    pub const LG: f32 = 12.0;
    /// size160
    pub const XL: f32 = 16.0;
}

/// Fluent corner radii.
/// <https://fluent2.microsoft.design/shapes>
#[allow(dead_code)]
pub mod radius {
    /// Navigation bars / edge-docked panels (no rounding at screen edge).
    pub const NONE: f32 = 0.0;
    /// Small badges / dense controls (&lt; 32px).
    pub const SMALL: f32 = 2.0;
    /// Buttons, dropdowns, icon buttons, preview tiles (default medium).
    pub const CONTROL: f32 = 4.0;
    /// Cards, raised surfaces, popovers.
    pub const CARD: f32 = 8.0;
    /// Dialogs / sheets.
    pub const DIALOG: f32 = 12.0;
}

/// Fluent stroke weights.
pub mod stroke {
    pub const THIN: f32 = 1.0;
    pub const THICK: f32 = 2.0;
}

/// Light theme: Fluent neutrals + shared semantic + AADT brand.
/// <https://fluent2.microsoft.design/color>
#[allow(dead_code)] // semantic / disabled tokens reserved for status UI
pub mod color {
    use eframe::egui::Color32;

    // --- Neutral palette (surfaces / text / layout) ---

    /// Recessed app chrome (neutral background).
    pub const LAYER_BASE: Color32 = Color32::from_rgb(243, 243, 243); // #F3F3F3
    /// Elevated card / panel surface.
    pub const LAYER_CARD: Color32 = Color32::from_rgb(255, 255, 255); // #FFFFFF
    /// Drawing paper — primary focus region.
    pub const CANVAS: Color32 = Color32::from_rgb(255, 255, 255);
    /// Control rest fill (slightly recessed vs card).
    pub const CONTROL: Color32 = Color32::from_rgb(243, 243, 243);
    /// Hover — Windows controls lighten/shift on interact; we darken neutrals slightly
    /// for clear rest→hover→pressed hierarchy on light chrome.
    pub const CONTROL_HOVER: Color32 = Color32::from_rgb(234, 234, 234);
    pub const CONTROL_PRESSED: Color32 = Color32::from_rgb(224, 224, 224);

    pub const TEXT: Color32 = Color32::from_rgb(36, 36, 36); // #242424
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(97, 97, 97); // #616161
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(161, 161, 161);

    pub const STROKE: Color32 = Color32::from_rgb(229, 229, 229); // #E5E5E5
    pub const DIVIDER: Color32 = Color32::from_rgb(209, 209, 209);
    pub const BORDER: Color32 = Color32::from_rgb(200, 200, 200);

    // --- Brand (AADT orange — sparingly) ---

    /// Brand orange — selection, focus, CTAs (`#E85D04`).
    pub const ACCENT: Color32 = Color32::from_rgb(232, 93, 4);
    /// Pressed brand (`#C2410C`).
    pub const ACCENT_DARK: Color32 = Color32::from_rgb(194, 65, 12);
    /// Soft brand fill (`#FFF0E6`).
    pub const ACCENT_SOFT: Color32 = Color32::from_rgb(255, 240, 230);

    // --- Shared semantic (status; never decoration-only) ---

    pub const SUCCESS: Color32 = Color32::from_rgb(15, 123, 50); // #0F7B32
    pub const WARNING: Color32 = Color32::from_rgb(157, 93, 0); // #9D5D00
    pub const DANGER: Color32 = Color32::from_rgb(196, 43, 28); // #C42B1C
    pub const INFO: Color32 = Color32::from_rgb(0, 120, 212); // #0078D4 — status only, not brand
}

/// Apply Fluent 2 visuals: neutrals, medium rounding, Windows-style stroke elevation
/// plus soft ambient shadow on floating surfaces.
pub fn apply_fluent_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.dark_mode = false;
    visuals.panel_fill = color::LAYER_BASE;
    visuals.window_fill = color::LAYER_CARD;
    visuals.extreme_bg_color = color::CONTROL;
    visuals.faint_bg_color = color::CONTROL;
    visuals.override_text_color = Some(color::TEXT);
    visuals.window_stroke = egui::Stroke::new(stroke::THIN, color::BORDER);

    // Soft ambient elevation for dialogs / floating menus (Fluent shadow 16-ish).
    visuals.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 4.0),
        blur: 16.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(28),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 2.0),
        blur: 8.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(36),
    };

    visuals.widgets.noninteractive.bg_fill = color::LAYER_BASE;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(stroke::THIN, color::STROKE);
    visuals.widgets.inactive.bg_fill = color::CONTROL;
    visuals.widgets.inactive.weak_bg_fill = color::CONTROL;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(stroke::THIN, color::STROKE);
    visuals.widgets.hovered.bg_fill = color::CONTROL_HOVER;
    visuals.widgets.hovered.weak_bg_fill = color::CONTROL_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(stroke::THIN, color::DIVIDER);
    visuals.widgets.active.bg_fill = color::CONTROL_PRESSED;
    visuals.widgets.active.weak_bg_fill = color::ACCENT_SOFT;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(stroke::THIN, color::ACCENT_DARK);
    visuals.widgets.open.bg_fill = color::LAYER_CARD;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(stroke::THIN, color::BORDER);
    visuals.selection.bg_fill = color::ACCENT_SOFT;
    visuals.selection.stroke = egui::Stroke::new(stroke::THICK, color::ACCENT);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(stroke::THIN, color::TEXT);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(stroke::THIN, color::TEXT);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(stroke::THIN, color::TEXT);

    // Medium control radius (4px); dialogs use larger via window_rounding.
    let control = egui::Rounding::same(radius::CONTROL);
    let dialog = egui::Rounding::same(radius::DIALOG);
    visuals.window_rounding = dialog;
    visuals.menu_rounding = egui::Rounding::same(radius::CARD);
    visuals.widgets.noninteractive.rounding = control;
    visuals.widgets.inactive.rounding = control;
    visuals.widgets.hovered.rounding = control;
    visuals.widgets.active.rounding = control;
    visuals.widgets.open.rounding = control;
    ctx.set_visuals(visuals);

    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Light));

    let mut style = (*ctx.style()).clone();
    // Fluent Windows type ramp (Caption / Body / Subtitle).
    // <https://fluent2.microsoft.design/typography>
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(12.0, egui::FontFamily::Proportional), // Caption
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional), // Body
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(20.0, egui::FontFamily::Proportional), // Subtitle
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
    );
    style.spacing.item_spacing = egui::vec2(space::MD, space::SM);
    style.spacing.button_padding = egui::vec2(space::MD, space::SM);
    style.spacing.window_margin = egui::Margin::same(space::LG);
    style.spacing.menu_margin = egui::Margin::same(space::MD);
    style.spacing.indent = space::LG;
    style.spacing.interact_size = egui::vec2(40.0, 24.0);
    style.spacing.icon_width = 16.0;
    style.spacing.icon_width_inner = 14.0;
    style.spacing.icon_spacing = space::SM;
    style.spacing.slider_width = 100.0;
    style.spacing.combo_width = 100.0;
    style.interaction.resize_grab_radius_side = 4.0;
    style.interaction.resize_grab_radius_corner = 6.0;
    // Motion: Fluent prefers short, purposeful transitions — egui animates hover softly.
    style.animation_time = 0.08;
    ctx.set_style(style);
}

/// Install UI fonts: Segoe UI on Windows (Fluent signature), Source Sans 3 as
/// fallback and as the drawing/annotation face.
pub fn install_drawing_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "SourceSans3".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/SourceSans3-Regular.ttf")),
    );
    fonts.font_data.insert(
        "SourceSans3Bold".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/SourceSans3-Bold.ttf")),
    );

    #[cfg(windows)]
    {
        if let Some(windir) = std::env::var_os("WINDIR") {
            let fonts_dir = std::path::PathBuf::from(windir).join("Fonts");
            for (name, file) in [
                ("SegoeUI", "segoeui.ttf"),
                ("SegoeUIBold", "segoeuib.ttf"),
            ] {
                let path = fonts_dir.join(file);
                if let Ok(bytes) = std::fs::read(&path) {
                    fonts
                        .font_data
                        .insert(name.to_owned(), egui::FontData::from_owned(bytes));
                }
            }
        }
    }

    fonts.families.insert(
        egui::FontFamily::Name("SourceSans3".into()),
        vec!["SourceSans3".to_owned(), "SourceSans3Bold".to_owned()],
    );

    // Proportional = chrome UI. Prefer Segoe when available.
    let mut prop = Vec::new();
    if fonts.font_data.contains_key("SegoeUI") {
        prop.push("SegoeUI".to_owned());
        if fonts.font_data.contains_key("SegoeUIBold") {
            prop.push("SegoeUIBold".to_owned());
        }
    }
    prop.push("SourceSans3".to_owned());
    prop.push("SourceSans3Bold".to_owned());
    fonts
        .families
        .insert(egui::FontFamily::Proportional, prop);

    if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        mono.push("SourceSans3".to_owned());
    }
    ctx.set_fonts(fonts);
}

/// Font for drawing / annotation text (Source Sans 3 — stable CAD face).
pub fn drawing_font(size_px: f32) -> egui::FontId {
    egui::FontId::new(
        size_px.max(1.0),
        egui::FontFamily::Name("SourceSans3".into()),
    )
}

/// Peer destinations in a side manager (flat / lateral nav).
pub struct NavDest {
    pub tab: usize,
    pub label: &'static str,
    pub tip: &'static str,
    pub icon: super::icons::Icon,
}

/// Left dock — structure / hierarchy (layers & blocks for the active space).
pub fn left_manager_nav() -> &'static [NavDest] {
    use super::icons::Icon;
    &[
        NavDest {
            tab: 0,
            label: "Layers",
            tip: "Layer properties (active space)",
            icon: Icon::Layers,
        },
        NavDest {
            tab: 1,
            label: "Blocks",
            tip: "Blocks for the active model space",
            icon: Icon::Blocks,
        },
    ]
}

/// Right dock — tools, quantities, and drawing details.
pub fn right_manager_nav() -> &'static [NavDest] {
    use super::icons::Icon;
    &[
        NavDest {
            tab: 0,
            label: "Takeoff",
            tip: "Quantities",
            icon: Icon::Takeoff,
        },
        NavDest {
            tab: 1,
            label: "Hatches",
            tip: "Hatch patterns",
            icon: Icon::Hatch,
        },
        NavDest {
            tab: 2,
            label: "Annotate",
            tip: "Annotation tools",
            icon: Icon::Annotate,
        },
        NavDest {
            tab: 3,
            label: "Templates",
            tip: "Nameplate template layers and fields",
            icon: Icon::Templates,
        },
        NavDest {
            tab: 4,
            label: "Units",
            tip: "Drawing units manager",
            icon: Icon::Props,
        },
        NavDest {
            tab: 5,
            label: "Dims",
            tip: "Dimension manager",
            icon: Icon::Dims,
        },
        NavDest {
            tab: 6,
            label: "History",
            tip: "Drawing time machine / versions",
            icon: Icon::History,
        },
        NavDest {
            tab: 7,
            label: "Properties",
            tip: "Selection / object properties",
            icon: Icon::ObjectProps,
        },
    ]
}

pub fn left_manager_title(tab: usize) -> &'static str {
    match tab {
        1 => "Blocks",
        _ => "Layers",
    }
}

pub fn right_manager_title(tab: usize) -> &'static str {
    match tab {
        1 => "Hatches",
        2 => "Annotate",
        3 => "Nameplate templates",
        4 => "Units",
        5 => "Dimensions",
        6 => "History",
        7 => "Properties",
        _ => "Takeoff",
    }
}

/// Horizontal peer nav — underline indicator (Fluent pivot/tab pattern).
pub fn nav_tabs(ui: &mut egui::Ui, current: &mut usize, destinations: &[NavDest]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = space::SM;
        for dest in destinations {
            let selected = *current == dest.tab;
            let text = if selected {
                egui::RichText::new(dest.label)
                    .strong()
                    .color(color::ACCENT)
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
                    egui::Stroke::new(stroke::THICK, color::ACCENT),
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
