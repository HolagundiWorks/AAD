//! **AAD Chrome** — mono design language for the egui shell.
//!
//! Grounded in [Fluent 2](https://fluent2.microsoft.design/design-principles) neutrals,
//! type, icons, and motion — with CAD-dense **sharp radii (0)** and brand orange used
//! sparingly for chrome focus/CTAs. Drafting overlays use the sibling [`canvas`] token
//! set (CAD-familiar blues/greens), not brand orange.
//!
//! Prefer the helpers in this module (`button`, `chip_*`, `dialog_frame`,
//! `section_label*`, …) over raw egui controls. See `docs/branding/README.md`.

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

    // Sharp corners (radius 0) everywhere — CAD-dense mono shape language.
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
    // Match panel CONTROL_H so default `ui.button` and sized CTAs share one height.
    style.spacing.interact_size = egui::vec2(40.0, panel_space::CONTROL_H);
    style.spacing.icon_width = icon::GLYPH_MD;
    style.spacing.icon_width_inner = icon::GLYPH_SM;
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
            label: "Elements",
            tip: "Elements (blocks) for the active model space",
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
            label: "Fills",
            tip: "Technical, floor, and presentation fills",
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
        1 => "Elements",
        _ => "Layers",
    }
}

pub fn right_manager_title(tab: usize) -> &'static str {
    match tab {
        1 => "Fills",
        2 => "Annotate",
        3 => "Nameplate templates",
        4 => "Units",
        5 => "Dimensions",
        6 => "History",
        7 => "Properties",
        _ => "Fills",
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
                    .font(semibold(type_scale::BODY))
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
    /// Full-width / primary control height — every dock CTA uses this.
    pub const CONTROL_H: f32 = 28.0;
    /// Prop / attribute label column height.
    pub const LABEL_H: f32 = 20.0;
    /// Icon rail width.
    pub const RAIL: f32 = 40.0;
    /// Layer color chip (dense row).
    pub const COLOR_CHIP_W: f32 = 24.0;
    pub const COLOR_CHIP_H: f32 = 12.0;
    /// ACI palette cell.
    #[allow(dead_code)]
    pub const PALETTE_CELL: f32 = 12.0;
    /// Preview tile size band (blocks / hatches).
    pub const TILE_MIN: f32 = 48.0;
    pub const TILE_MAX: f32 = 88.0;
    pub const TILE_MIN_BLOCKS: f32 = 52.0;
    pub const TILE_MAX_BLOCKS: f32 = 96.0;
}

/// Icon glyph and hit-target sizes (Fluent System Icons UI steps).
pub mod icon {
    /// Toolbar + nav glyph (matches `style.spacing.icon_width`).
    pub const GLYPH_MD: f32 = 16.0;
    /// Dense list glyph (matches `style.spacing.icon_width_inner`).
    pub const GLYPH_SM: f32 = 14.0;
    /// Toolbar / QAT icon hit target.
    pub const HIT_TOOLBAR: f32 = 28.0;
    /// Nav rail icon hit target — same as toolbar for visual parity.
    pub const HIT_NAV: f32 = 28.0;
    /// Layer-row compact icon hit target.
    pub const HIT_COMPACT: f32 = 20.0;
    /// Brand mark in the top bar (logo guidelines: 20–24).
    pub const BRAND: f32 = 20.0;
}

/// Top / bottom chrome bar heights.
pub mod chrome {
    /// Quick-access + levels strips.
    pub const TOP_BAR_H: f32 = 32.0;
    /// Collapsed status / command bar.
    pub const STATUS_H: f32 = 28.0;
    /// Expanded status bar (hint + completions).
    pub const STATUS_H_EXP: f32 = 48.0;
}

/// Keep chrome inside the OS client area (never paint over the taskbar / off-window).
pub mod layout {
    use super::{chrome, panel_space};

    /// Minimum usable OS window client size.
    pub const MIN_WINDOW_W: f32 = 720.0;
    pub const MIN_WINDOW_H: f32 = 480.0;

    /// QAT + levels strip.
    pub fn top_chrome_h() -> f32 {
        chrome::TOP_BAR_H * 2.0
    }

    /// Height available for edge overlays between top chrome and status pad.
    pub fn overlay_height(screen: egui::Rect, bottom_pad: f32) -> f32 {
        (screen.height() - top_chrome_h() - bottom_pad).max(80.0)
    }

    /// Max height for the drawing-properties bottom sheet.
    pub fn drawing_props_max_h(screen: egui::Rect) -> f32 {
        let leave = top_chrome_h() + chrome::STATUS_H_EXP + 160.0;
        ((screen.height() - leave).max(120.0)).min(screen.height() * 0.5)
    }

    /// Max height for modal/dialog windows.
    pub fn dialog_max_h(screen: egui::Rect) -> f32 {
        (screen.height() * 0.85).max(200.0)
    }

    /// Dock width that still leaves a usable canvas.
    pub fn dock_width(preferred: f32, screen_w: f32, both_docks: bool) -> f32 {
        let canvas_min = 240.0;
        let rails = panel_space::RAIL * 2.0;
        let budget = if both_docks {
            ((screen_w - canvas_min - rails) * 0.5).max(120.0)
        } else {
            (screen_w - canvas_min - rails).max(120.0)
        };
        preferred.min(budget).clamp(120.0, 340.0)
    }

    /// Clamp a floating panel rect into `screen`.
    pub fn clamp_float_rect(
        x: &mut f32,
        y: &mut f32,
        w: &mut f32,
        h: &mut f32,
        screen: egui::Rect,
    ) {
        let max_w = screen.width().clamp(200.0, 640.0);
        let max_h = overlay_height(screen, chrome::STATUS_H).min(1200.0);
        *w = (*w).clamp(200.0, max_w);
        *h = (*h).clamp(160.0, max_h.max(160.0));
        let max_x = (screen.right() - *w).max(screen.left());
        let max_y = (screen.bottom() - *h).max(screen.top());
        *x = (*x).clamp(screen.left(), max_x);
        *y = (*y).clamp(screen.top(), max_y);
    }

    /// Clamp a fixed overlay position so a `size` box stays on screen.
    pub fn clamp_pos(pos: egui::Pos2, size: egui::Vec2, screen: egui::Rect) -> egui::Pos2 {
        egui::pos2(
            pos.x
                .clamp(screen.left(), (screen.right() - size.x).max(screen.left())),
            pos.y
                .clamp(screen.top(), (screen.bottom() - size.y).max(screen.top())),
        )
    }
}

/// AAD Canvas — drafting overlay colors (CAD muscle memory, not brand).
///
/// Chrome selection/focus stays [`color::accent`]; model-space highlights use
/// these blues/greens so AutoCAD-trained hands stay oriented.
pub mod canvas {
    use eframe::egui::Color32;

    /// Primary selection / grip highlight.
    pub fn selection() -> Color32 {
        Color32::from_rgb(30, 120, 255)
    }

    /// Darker selection stroke (previews, guides).
    pub fn selection_stroke() -> Color32 {
        Color32::from_rgb(30, 90, 180)
    }

    /// Soft selection wash.
    #[allow(dead_code)]
    pub fn selection_fill() -> Color32 {
        Color32::from_rgba_unmultiplied(30, 120, 255, 40)
    }

    /// Soft glow around grips / hover highlights.
    pub fn selection_glow() -> Color32 {
        Color32::from_rgba_unmultiplied(40, 140, 255, 110)
    }

    /// Plate behind selected text labels.
    pub fn selection_plate() -> Color32 {
        Color32::from_rgba_unmultiplied(30, 120, 255, 60)
    }

    /// Rubber-band / ghost preview stroke (in-progress geometry).
    pub fn ghost() -> Color32 {
        Color32::from_rgb(80, 140, 220)
    }

    /// TRIM preview — segment about to be removed.
    pub fn trim_ghost() -> Color32 {
        Color32::from_rgba_unmultiplied(230, 70, 70, 210)
    }

    /// Grip fill (on selection handles).
    pub fn grip() -> Color32 {
        Color32::from_rgb(255, 255, 255)
    }

    /// UCS X axis.
    pub fn ucs_x() -> Color32 {
        Color32::from_rgb(200, 40, 40)
    }

    /// UCS Y axis.
    pub fn ucs_y() -> Color32 {
        Color32::from_rgb(40, 160, 40)
    }

    /// Shared building reference-origin mark.
    pub fn reference_mark() -> Color32 {
        Color32::from_rgb(180, 80, 20)
    }

    /// Window box (L→R) edge / fill.
    pub fn window_edge() -> Color32 {
        Color32::from_rgb(40, 100, 220)
    }
    pub fn window_fill() -> Color32 {
        Color32::from_rgba_unmultiplied(40, 100, 220, 28)
    }

    /// Crossing box (R→L) edge / fill.
    pub fn crossing_edge() -> Color32 {
        Color32::from_rgb(40, 180, 60)
    }
    pub fn crossing_fill() -> Color32 {
        Color32::from_rgba_unmultiplied(40, 180, 60, 28)
    }

    /// Object-snap marker.
    pub fn snap() -> Color32 {
        Color32::from_rgb(255, 200, 60)
    }

    /// Crosshair ink (theme-aware).
    pub fn crosshair() -> Color32 {
        if super::color::is_dark() {
            Color32::from_rgb(200, 200, 200)
        } else {
            Color32::from_rgb(60, 60, 60)
        }
    }

    /// Pickbox stroke (theme-aware).
    pub fn pickbox() -> Color32 {
        if super::color::is_dark() {
            Color32::from_rgb(225, 225, 225)
        } else {
            Color32::from_rgb(20, 20, 20)
        }
    }
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

/// Chrome button variants — use these instead of raw `egui::Button` in docks/bars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BtnKind {
    /// Solid brand orange — primary CTA.
    Primary,
    /// Neutral fill + border — default dock action.
    Secondary,
    /// Transparent / weak — tertiary.
    Subtle,
    /// Danger-colored — destructive.
    Danger,
}

fn btn_style(kind: BtnKind) -> (egui::Color32, egui::Color32, egui::Color32) {
    match kind {
        BtnKind::Primary => (color::accent(), color::accent_dark(), egui::Color32::WHITE),
        BtnKind::Secondary => (color::control(), color::stroke(), color::text()),
        BtnKind::Subtle => (egui::Color32::TRANSPARENT, color::stroke(), color::text()),
        BtnKind::Danger => (
            egui::Color32::from_rgba_unmultiplied(
                color::danger().r(),
                color::danger().g(),
                color::danger().b(),
                40,
            ),
            color::danger(),
            color::danger(),
        ),
    }
}

/// Sized chrome button at [`panel_space::CONTROL_H`].
pub fn button(ui: &mut egui::Ui, kind: BtnKind, label: &str) -> egui::Response {
    let (fill, stroke_c, text_c) = btn_style(kind);
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(type_scale::BODY)
                .color(text_c),
        )
        .fill(fill)
        .stroke(egui::Stroke::new(stroke::THIN, stroke_c))
        .rounding(radius::CONTROL),
    )
}

/// Gated chrome button — disabled state uses egui's enabled overlay.
pub fn button_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    kind: BtnKind,
    label: &str,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| button(ui, kind, label))
        .inner
}

/// Dense tertiary link (replaces `ui.small_button` in chrome).
pub fn link_button(ui: &mut egui::Ui, label: impl Into<String>) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label.into())
                .size(type_scale::CAPTION)
                .color(color::text_secondary()),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .rounding(radius::CONTROL),
    )
}

/// Fixed-size chrome button (width × [`panel_space::CONTROL_H`]).
pub fn button_sized(ui: &mut egui::Ui, kind: BtnKind, label: &str, width: f32) -> egui::Response {
    let (fill, stroke_c, text_c) = btn_style(kind);
    ui.add_sized(
        [width, panel_space::CONTROL_H],
        egui::Button::new(
            egui::RichText::new(label)
                .size(type_scale::BODY)
                .color(text_c),
        )
        .fill(fill)
        .stroke(egui::Stroke::new(stroke::THIN, stroke_c))
        .rounding(radius::CONTROL),
    )
}

/// Full-width secondary action button (primary stack).
pub fn full_width_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    button_sized(ui, BtnKind::Secondary, label, ui.available_width())
}

/// Two equal secondary columns on the 4px grid.
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
        left_r = Some(button_sized(ui, BtnKind::Secondary, left, w));
        right_r = Some(button_sized(ui, BtnKind::Secondary, right, w));
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

/// Accent chip (units token) — clickable brand soft fill.
pub fn chip_accent(ui: &mut egui::Ui, label: &str, tip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .font(semibold(type_scale::CAPTION))
                .color(color::accent()),
        )
        .fill(color::accent_soft())
        .stroke(egui::Stroke::new(stroke::THIN, color::accent()))
        .rounding(radius::CONTROL),
    )
    .on_hover_text(tip)
}

/// Toggle chip (ORTHO / OSNAP / …) — on = accent soft, off = control fill.
pub fn chip_toggle(ui: &mut egui::Ui, label: &str, on: bool, tip: &str) -> egui::Response {
    let text = if on {
        egui::RichText::new(label)
            .font(semibold(type_scale::CAPTION))
            .color(color::text())
    } else {
        egui::RichText::new(label)
            .size(type_scale::CAPTION)
            .color(color::text_secondary())
    };
    ui.add(
        egui::Button::new(text)
            .fill(if on {
                color::accent_soft()
            } else {
                color::control()
            })
            .stroke(egui::Stroke::new(
                stroke::THIN,
                if on { color::accent() } else { color::stroke() },
            ))
            .rounding(radius::CONTROL),
    )
    .on_hover_text(tip)
}

/// Semantic note line under the status / command bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Warn/Error reserved for validation / failures.
pub enum NoteKind {
    /// Active tool prompt — accent.
    Prompt,
    Info,
    Warn,
    Error,
}

/// One-line status / prompt feedback.
pub fn note_line(ui: &mut egui::Ui, kind: NoteKind, text: impl Into<String>) {
    let c = match kind {
        NoteKind::Prompt => color::accent(),
        NoteKind::Info => color::info(),
        NoteKind::Warn => color::warning(),
        NoteKind::Error => color::danger(),
    };
    ui.label(
        egui::RichText::new(text.into())
            .size(type_scale::CAPTION)
            .color(c),
    );
}

/// Shared frame for modal `egui::Window`s.
pub fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.style())
        .fill(color::layer_card())
        .stroke(egui::Stroke::new(stroke::THIN, color::border()))
        .rounding(radius::DIALOG)
        .shadow(elevation::modal())
}

/// Frame + screen clamp helpers for dialogs.
pub fn dialog_defaults(ctx: &egui::Context) -> (egui::Frame, egui::Rect, f32) {
    (
        dialog_frame(ctx),
        ctx.screen_rect(),
        layout::dialog_max_h(ctx.screen_rect()),
    )
}

/// Docked panel surface (stroke only — no shadow).
pub fn dock_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(color::layer_card())
        .inner_margin(egui::Margin::symmetric(panel_space::GUTTER, space::MD))
        .stroke(egui::Stroke::new(stroke::THIN, color::border()))
        .shadow(elevation::dock())
}

/// Floating / autohide dock — same fill as [`dock_frame`] plus directional shadow.
pub fn float_dock_frame(offset: egui::Vec2) -> egui::Frame {
    dock_frame().shadow(elevation::float(offset))
}

/// Popup / context menu surface (snap override, etc.).
pub fn menu_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(color::layer_card())
        .inner_margin(egui::Margin::symmetric(space::SM, space::SM))
        .stroke(egui::Stroke::new(stroke::THIN, color::border()))
        .rounding(radius::CONTROL)
        .shadow(elevation::popup())
}

/// Fluent-inspired elevation shadows — Bar/Dock = stroke only; Float/Modal lift.
pub mod elevation {
    use eframe::egui;

    fn alpha(float: u8, dark_boost: u8) -> u8 {
        if super::color::is_dark() {
            float.saturating_add(dark_boost)
        } else {
            float
        }
    }

    /// No lift (bars, docked panels — edge via stroke).
    pub fn none() -> egui::epaint::Shadow {
        egui::epaint::Shadow {
            offset: egui::vec2(0.0, 0.0),
            blur: 0.0,
            spread: 0.0,
            color: egui::Color32::TRANSPARENT,
        }
    }

    /// Alias for [`none`] — QAT / status.
    pub fn bar() -> egui::epaint::Shadow {
        none()
    }

    /// Alias for [`none`] — docked manager.
    pub fn dock() -> egui::epaint::Shadow {
        none()
    }

    /// Floating / autohide panel. `offset` biases the key shadow (e.g. slide-out).
    pub fn float(offset: egui::Vec2) -> egui::epaint::Shadow {
        egui::epaint::Shadow {
            offset,
            blur: 12.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(alpha(40, 36)),
        }
    }

    /// Context menus / transient overlays.
    pub fn popup() -> egui::epaint::Shadow {
        egui::epaint::Shadow {
            offset: egui::vec2(0.0, 4.0),
            blur: 8.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(alpha(32, 48)),
        }
    }

    /// Modal dialogs.
    pub fn modal() -> egui::epaint::Shadow {
        egui::epaint::Shadow {
            offset: egui::vec2(0.0, 8.0),
            blur: 20.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(alpha(30, 60)),
        }
    }
}

/// Label + full-width edit — wraps vertically when the panel is narrow.
pub fn prop_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let w = ui.available_width();
    if w < 220.0 {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(type_scale::CAPTION)
                    .color(color::text_secondary()),
            );
            ui.add(egui::TextEdit::singleline(value).desired_width(ui.available_width()));
        });
    } else {
        ui.horizontal(|ui| {
            let label_w = (w * 0.32).clamp(64.0, 110.0);
            ui.add_sized(
                [label_w, panel_space::LABEL_H],
                egui::Label::new(
                    egui::RichText::new(label)
                        .size(type_scale::CAPTION)
                        .color(color::text_secondary()),
                ),
            );
            ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
        });
    }
}

/// Selectable preview tile chrome (hatch / block / linetype grids).
pub fn tile_frame(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    selected: bool,
) -> (egui::Rect, egui::Response) {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    let bg = if selected {
        color::accent_soft()
    } else {
        color::canvas()
    };
    let edge = if selected {
        egui::Stroke::new(stroke::THICK, color::accent())
    } else {
        egui::Stroke::new(stroke::THIN, color::border())
    };
    painter.rect_filled(rect, radius::CONTROL, bg);
    painter.rect_stroke(rect, radius::CONTROL, edge);
    (rect, resp)
}

/// Layer color chip (dense row) — click opens the ACI palette.
pub fn color_swatch(ui: &mut egui::Ui, fill: egui::Color32, tip: &str) -> egui::Response {
    let (resp, painter) = ui.allocate_painter(
        egui::vec2(panel_space::COLOR_CHIP_W, panel_space::COLOR_CHIP_H),
        egui::Sense::click(),
    );
    painter.rect_filled(resp.rect, radius::SMALL, fill);
    painter.rect_stroke(
        resp.rect,
        radius::SMALL,
        egui::Stroke::new(stroke::THIN, color::stroke()),
    );
    resp.on_hover_text(tip)
}

/// Peer nav caption beside an icon rail button.
pub fn nav_dest_label(ui: &mut egui::Ui, label: &str, selected: bool) {
    ui.label(
        egui::RichText::new(label)
            .font(semibold(type_scale::CAPTION))
            .color(if selected {
                color::accent()
            } else {
                color::text_secondary()
            }),
    );
}

/// Top / bottom bar surface frame.
pub fn bar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(color::layer_base())
        .inner_margin(egui::Margin::symmetric(space::SM, space::XS))
        .stroke(egui::Stroke::new(stroke::THIN, color::border()))
        .shadow(elevation::bar())
}
