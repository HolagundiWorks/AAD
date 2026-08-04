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

/// Corner radii. Squared off (all `0.0`) — the UI uses sharp corners on every
/// button, control, card, and dialog. Kept as named tokens (rather than
/// inlining `0.0`) so a future rounding scheme is a one-file change.
#[allow(dead_code)]
pub mod radius {
    /// Navigation bars / edge-docked panels.
    pub const NONE: f32 = 0.0;
    /// Small badges / dense controls (&lt; 32px).
    pub const SMALL: f32 = 0.0;
    /// Buttons, dropdowns, icon buttons, preview tiles.
    pub const CONTROL: f32 = 0.0;
    /// Cards, raised surfaces, popovers.
    pub const CARD: f32 = 0.0;
    /// Dialogs / sheets.
    pub const DIALOG: f32 = 0.0;
}

/// Fluent stroke weights.
pub mod stroke {
    pub const THIN: f32 = 1.0;
    pub const THICK: f32 = 2.0;
}

/// Chrome appearance — Light (default) or Dark. Persisted in `UserPrefs`
/// and installed via [`apply_fluent_visuals`]; drives both the egui `Visuals`
/// and the runtime [`color`] palette the custom painters read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    pub fn is_dark(self) -> bool {
        matches!(self, Theme::Dark)
    }

    /// Human label for the toggle tooltip / menu.
    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }

    /// The other theme (for a toggle).
    pub fn toggled(self) -> Theme {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }

    /// Token palette backing the custom painters for this theme.
    pub fn palette(self) -> color::Palette {
        match self {
            Theme::Light => color::Palette::LIGHT,
            Theme::Dark => color::Palette::DARK,
        }
    }
}

impl From<bool> for Theme {
    /// `true` → dark (the persisted `UserPrefs::dark_theme` flag).
    fn from(dark: bool) -> Self {
        if dark {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
}

/// Fluent neutrals + shared semantic + AADT brand, resolved for the active
/// [`Theme`] at runtime.
///
/// The tokens are accessed as functions (`color::text()`, …) rather than
/// constants: each reads the [`Palette`] installed by [`apply_fluent_visuals`]
/// from a thread-local (egui runs the whole UI on one thread), so a theme
/// switch recolors every custom-painted surface without touching call sites.
/// <https://fluent2.microsoft.design/color>
#[allow(dead_code)] // semantic / disabled tokens reserved for status UI
pub mod color {
    use eframe::egui::Color32;
    use std::cell::Cell;

    /// A complete set of chrome tokens for one theme.
    #[derive(Clone, Copy)]
    pub struct Palette {
        pub layer_base: Color32,
        pub layer_card: Color32,
        pub canvas: Color32,
        pub control: Color32,
        pub control_hover: Color32,
        pub control_pressed: Color32,
        pub text: Color32,
        pub text_secondary: Color32,
        pub text_disabled: Color32,
        pub stroke: Color32,
        pub divider: Color32,
        pub border: Color32,
        pub accent: Color32,
        pub accent_dark: Color32,
        pub accent_soft: Color32,
        pub success: Color32,
        pub warning: Color32,
        pub danger: Color32,
        pub info: Color32,
        /// Ink for ByLayer / black (ACI 0·7) geometry, so default lines stay
        /// visible when the paper goes dark. Light: black; Dark: near-white.
        pub ink: Color32,
    }

    impl Palette {
        /// Light theme: Fluent neutrals on white paper, black ink.
        pub const LIGHT: Palette = Palette {
            layer_base: Color32::from_rgb(243, 243, 243), // #F3F3F3
            layer_card: Color32::from_rgb(255, 255, 255), // #FFFFFF
            canvas: Color32::from_rgb(255, 255, 255),     // white paper
            control: Color32::from_rgb(243, 243, 243),
            control_hover: Color32::from_rgb(234, 234, 234),
            control_pressed: Color32::from_rgb(224, 224, 224),
            text: Color32::from_rgb(36, 36, 36), // #242424
            text_secondary: Color32::from_rgb(97, 97, 97), // #616161
            text_disabled: Color32::from_rgb(161, 161, 161),
            stroke: Color32::from_rgb(229, 229, 229), // #E5E5E5
            divider: Color32::from_rgb(209, 209, 209),
            border: Color32::from_rgb(200, 200, 200),
            accent: Color32::from_rgb(232, 93, 4), // #E85D04
            accent_dark: Color32::from_rgb(194, 65, 12), // #C2410C
            accent_soft: Color32::from_rgb(255, 240, 230), // #FFF0E6
            success: Color32::from_rgb(15, 123, 50), // #0F7B32
            warning: Color32::from_rgb(157, 93, 0), // #9D5D00
            danger: Color32::from_rgb(196, 43, 28), // #C42B1C
            info: Color32::from_rgb(15, 108, 189), // #0F6CBD
            ink: Color32::from_rgb(0, 0, 0),
        };

        /// Dark theme: Fluent 2 dark neutrals on a dark model space, near-white
        /// ink. Brand orange is lifted slightly and its soft fill goes warm-dark
        /// so it still reads on the dark chrome; status hues are lightened for
        /// contrast (AA on the dark surfaces).
        pub const DARK: Palette = Palette {
            layer_base: Color32::from_rgb(32, 32, 32),        // #202020
            layer_card: Color32::from_rgb(43, 43, 43),        // #2B2B2B
            canvas: Color32::from_rgb(30, 30, 30),            // #1E1E1E dark paper
            control: Color32::from_rgb(50, 50, 50),           // #323232
            control_hover: Color32::from_rgb(61, 61, 61),     // #3D3D3D
            control_pressed: Color32::from_rgb(71, 71, 71),   // #474747
            text: Color32::from_rgb(255, 255, 255),           // #FFFFFF
            text_secondary: Color32::from_rgb(200, 200, 200), // #C8C8C8
            text_disabled: Color32::from_rgb(122, 122, 122),
            stroke: Color32::from_rgb(64, 64, 64),   // #404040
            divider: Color32::from_rgb(77, 77, 77),  // #4D4D4D
            border: Color32::from_rgb(87, 87, 87),   // #575757
            accent: Color32::from_rgb(240, 122, 46), // #F07A2E (lifted brand)
            accent_dark: Color32::from_rgb(194, 65, 12), // #C2410C
            accent_soft: Color32::from_rgb(61, 43, 30), // warm-dark fill
            success: Color32::from_rgb(108, 203, 142),
            warning: Color32::from_rgb(240, 179, 90),
            danger: Color32::from_rgb(255, 153, 141),
            info: Color32::from_rgb(118, 185, 237),
            ink: Color32::from_rgb(230, 230, 230), // near-white ink
        };
    }

    thread_local! {
        static CURRENT: Cell<Palette> = const { Cell::new(Palette::LIGHT) };
    }

    /// Install `p` as the palette every accessor reads. Call on startup and on
    /// each theme switch (from [`super::apply_fluent_visuals`]).
    pub fn set(p: Palette) {
        CURRENT.with(|c| c.set(p));
    }

    /// The palette currently in effect.
    pub fn current() -> Palette {
        CURRENT.with(|c| c.get())
    }

    /// Whether the dark palette is active — for custom painters that need to
    /// pick a contrasting ink against a theme-filled surface.
    pub fn is_dark() -> bool {
        current().layer_base.r() < 128
    }

    /// Recessed app chrome (neutral background).
    pub fn layer_base() -> Color32 {
        current().layer_base
    }
    /// Elevated card / panel surface.
    pub fn layer_card() -> Color32 {
        current().layer_card
    }
    /// Drawing paper — primary focus region.
    pub fn canvas() -> Color32 {
        current().canvas
    }
    /// Control rest fill (slightly recessed vs card).
    pub fn control() -> Color32 {
        current().control
    }
    pub fn control_hover() -> Color32 {
        current().control_hover
    }
    pub fn control_pressed() -> Color32 {
        current().control_pressed
    }
    pub fn text() -> Color32 {
        current().text
    }
    pub fn text_secondary() -> Color32 {
        current().text_secondary
    }
    pub fn text_disabled() -> Color32 {
        current().text_disabled
    }
    /// Subtle 1px separators on controls.
    pub fn stroke() -> Color32 {
        current().stroke
    }
    pub fn divider() -> Color32 {
        current().divider
    }
    pub fn border() -> Color32 {
        current().border
    }
    /// Brand orange — selection, focus, CTAs.
    pub fn accent() -> Color32 {
        current().accent
    }
    /// Pressed brand.
    pub fn accent_dark() -> Color32 {
        current().accent_dark
    }
    /// Soft brand fill.
    pub fn accent_soft() -> Color32 {
        current().accent_soft
    }
    pub fn success() -> Color32 {
        current().success
    }
    pub fn warning() -> Color32 {
        current().warning
    }
    pub fn danger() -> Color32 {
        current().danger
    }
    pub fn info() -> Color32 {
        current().info
    }
    /// Ink for ByLayer / black geometry on the (possibly dark) canvas.
    pub fn ink() -> Color32 {
        current().ink
    }
}

/// Apply Fluent 2 visuals for `theme`: neutrals, medium rounding, Windows-style
/// stroke elevation plus soft ambient shadow on floating surfaces. Installs the
/// matching [`color`] palette first so both egui and the custom painters agree.
pub fn apply_fluent_visuals(ctx: &egui::Context, theme: Theme) {
    color::set(theme.palette());
    let dark = theme.is_dark();
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.dark_mode = dark;
    visuals.panel_fill = color::layer_base();
    visuals.window_fill = color::layer_card();
    visuals.extreme_bg_color = color::control();
    visuals.faint_bg_color = color::control();
    visuals.override_text_color = Some(color::text());
    visuals.window_stroke = egui::Stroke::new(stroke::THIN, color::border());

    // Elevation. Fluent on Windows outlines surfaces with strokes and keeps key
    // shadows subtle (see window_stroke / widget bg_strokes below). Dialogs sit
    // higher (≈ shadow28: y8/blur20), menus & tooltips lower (≈ shadow8: y4/blur8).
    // <https://fluent2.microsoft.design/elevation>
    // Dark surfaces need a deeper key shadow to separate from the chrome.
    let (win_alpha, pop_alpha) = if dark { (90, 96) } else { (30, 32) };
    visuals.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 8.0),
        blur: 20.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(win_alpha),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 4.0),
        blur: 8.0,
        spread: 0.0,
        color: egui::Color32::from_black_alpha(pop_alpha),
    };

    visuals.widgets.noninteractive.bg_fill = color::layer_base();
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(stroke::THIN, color::stroke());
    visuals.widgets.inactive.bg_fill = color::control();
    visuals.widgets.inactive.weak_bg_fill = color::control();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(stroke::THIN, color::stroke());
    visuals.widgets.hovered.bg_fill = color::control_hover();
    visuals.widgets.hovered.weak_bg_fill = color::control_hover();
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(stroke::THIN, color::divider());
    visuals.widgets.active.bg_fill = color::control_pressed();
    visuals.widgets.active.weak_bg_fill = color::accent_soft();
    visuals.widgets.active.bg_stroke = egui::Stroke::new(stroke::THIN, color::accent_dark());
    visuals.widgets.open.bg_fill = color::layer_card();
    visuals.widgets.open.bg_stroke = egui::Stroke::new(stroke::THIN, color::border());
    visuals.selection.bg_fill = color::accent_soft();
    visuals.selection.stroke = egui::Stroke::new(stroke::THICK, color::accent());
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(stroke::THIN, color::text());
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(stroke::THIN, color::text());
    visuals.widgets.active.fg_stroke = egui::Stroke::new(stroke::THIN, color::text());

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

    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(if dark {
        egui::SystemTheme::Dark
    } else {
        egui::SystemTheme::Light
    }));

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
    // Motion: Fluent prefers short, purposeful transitions. 100ms ≈ Fluent
    // `durationFaster`, the band used for hover/rest state changes.
    // <https://fluent2.microsoft.design/motion>
    style.animation_time = 0.10;
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
            // Segoe UI is Fluent's primary typeface; Semibold carries "Strong"
            // (Fluent weight 600) for headings and emphasis.
            for (name, file) in [
                ("SegoeUI", "segoeui.ttf"),
                ("SegoeUISemibold", "seguisb.ttf"),
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

    // "Semibold" family — Fluent's "Strong" weight for titles/section labels.
    // Prefer Segoe UI Semibold; fall back to Source Sans 3 Bold off-Windows.
    let mut semibold = Vec::new();
    if fonts.font_data.contains_key("SegoeUISemibold") {
        semibold.push("SegoeUISemibold".to_owned());
    }
    semibold.push("SourceSans3Bold".to_owned());
    fonts
        .families
        .insert(egui::FontFamily::Name("Semibold".into()), semibold);

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
    fonts.families.insert(egui::FontFamily::Proportional, prop);

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
        _ => "Hatches",
    }
}

/// Horizontal peer nav — underline indicator (Fluent pivot/tab pattern).
/// Kept for dense text pivots; docks prefer the icon rail.
#[allow(dead_code)]
pub fn nav_tabs(ui: &mut egui::Ui, current: &mut usize, destinations: &[NavDest]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = space::SM;
        for dest in destinations {
            let selected = *current == dest.tab;
            let text = if selected {
                egui::RichText::new(dest.label)
                    .strong()
                    .color(color::accent())
            } else {
                egui::RichText::new(dest.label).color(color::text_secondary())
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
                    egui::Stroke::new(stroke::THICK, color::accent()),
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

// --- Side-panel composition (Fluent layout + type ramp) ---------------------

/// Fluent type sizes used inside docks (Caption / Body / panel title).
pub mod type_scale {
    pub const CAPTION: f32 = 12.0;
    pub const BODY: f32 = 14.0;
    /// Panel header — between Body and Subtitle so docks stay compact.
    pub const TITLE: f32 = 16.0;
}

/// Vertical rhythm inside a side panel (4px grid).
pub mod panel_space {
    use super::space;
    /// Outer content inset already on the SidePanel frame; use this inside ScrollArea.
    pub const GUTTER: f32 = space::LG; // 12
    /// Gap between major sections.
    pub const SECTION: f32 = space::XL; // 16
    /// Gap between rows inside a section.
    pub const ROW: f32 = space::MD; // 8
    /// Tight gap (label → field).
    pub const TIGHT: f32 = space::SM; // 4
    /// Full-width control height.
    pub const CONTROL_H: f32 = 28.0;
    /// Icon rail width.
    pub const RAIL: f32 = 40.0;
}

/// Fluent "Strong" weight (Semibold 600) at `size_px` — for titles and section
/// labels. `.strong()` in egui only recolors; this switches to the Semibold face.
/// <https://fluent2.microsoft.design/typography>
pub fn semibold(size_px: f32) -> egui::FontId {
    egui::FontId::new(size_px.max(1.0), egui::FontFamily::Name("Semibold".into()))
}

/// Panel dock title (one per manager).
pub fn panel_title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(semibold(type_scale::TITLE))
            .color(color::text()),
    );
}

/// One-line context under the title (floor · space, etc.).
pub fn panel_subtitle(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(
        egui::RichText::new(text.into())
            .size(type_scale::CAPTION)
            .color(color::text_secondary()),
    );
}

/// Section label — starts a new block; call at the top of each group.
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(panel_space::SECTION);
    ui.label(
        egui::RichText::new(text)
            .font(semibold(type_scale::CAPTION))
            .color(color::text_secondary()),
    );
    ui.add_space(panel_space::TIGHT);
}

/// First section in a panel (no leading gap).
pub fn section_label_first(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(semibold(type_scale::CAPTION))
            .color(color::text_secondary()),
    );
    ui.add_space(panel_space::TIGHT);
}

/// Supporting caption under a section or control.
pub fn section_hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(
        egui::RichText::new(text.into())
            .size(type_scale::CAPTION)
            .color(color::text_secondary()),
    );
    ui.add_space(panel_space::TIGHT);
}

/// Hairline between sections when a label alone is not enough.
#[allow(dead_code)]
pub fn section_rule(ui: &mut egui::Ui) {
    ui.add_space(panel_space::ROW);
    let y = ui.cursor().top();
    let w = ui.available_width();
    ui.painter().hline(
        ui.max_rect().left()..=ui.max_rect().left() + w,
        y,
        egui::Stroke::new(stroke::THIN, color::divider()),
    );
    ui.add_space(panel_space::ROW);
}

/// Full-width action button (primary stack).
pub fn full_width_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), panel_space::CONTROL_H],
        egui::Button::new(egui::RichText::new(label).size(type_scale::BODY)),
    )
}

/// Two equal columns on the 4px grid.
pub fn two_col_buttons(
    ui: &mut egui::Ui,
    left: &str,
    right: &str,
) -> (egui::Response, egui::Response) {
    let gap = panel_space::ROW;
    let total = ui.available_width();
    let w = ((total - gap) * 0.5).max(72.0);
    let mut left_r = None;
    let mut right_r = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        left_r = Some(ui.add_sized(
            [w, panel_space::CONTROL_H],
            egui::Button::new(egui::RichText::new(left).size(type_scale::BODY)),
        ));
        right_r = Some(ui.add_sized(
            [w, panel_space::CONTROL_H],
            egui::Button::new(egui::RichText::new(right).size(type_scale::BODY)),
        ));
    });
    (left_r.unwrap(), right_r.unwrap())
}

/// Full-width single-line field.
pub fn full_width_edit(ui: &mut egui::Ui, value: &mut String, hint: &str) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(ui.available_width())
            .hint_text(hint),
    )
}
