//! The typography registry: five font presets (each a coordinated Sans, Condensed and Mono), the
//! semantic roles the interface and drawings ask for, and how a role resolves to an installed font
//! with a deterministic fallback. No font file is part of this crate or of the repository: the fonts
//! are an optional build and packaging input (the installer places their files, with their licences,
//! in the application `fonts` folder, which [`crate::ttf`] searches before the system folders). A
//! preset whose fonts are not installed resolves to the next available face and says so; it never
//! silently looks like it worked.

use crate::ttf;

/// The three faces of a preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Face {
    Sans,
    Condensed,
    Mono,
}

/// What a piece of text is for. Roles may share a face; nothing here adds a typeface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// Dialogs, forms, notes and general annotations.
    Primary,
    /// Toolbar labels, compact metadata, dense schedules, small title-block fields.
    Condensed,
    /// The command bar, coordinates, identifiers, codes and tabular values.
    Monospace,
    /// Page titles, sheet titles, drawing headings.
    Heading,
    /// Dimensions, levels and measurement labels.
    Dimension,
    /// Tables and schedules.
    Schedule,
    /// Company and project information, title blocks.
    Nameplate,
}

pub const ROLES: [Role; 7] = [Role::Primary, Role::Condensed, Role::Monospace, Role::Heading, Role::Dimension, Role::Schedule, Role::Nameplate];

impl Role {
    pub fn id(self) -> &'static str {
        match self {
            Role::Primary => "primary",
            Role::Condensed => "condensed",
            Role::Monospace => "monospace",
            Role::Heading => "heading",
            Role::Dimension => "dimension",
            Role::Schedule => "schedule",
            Role::Nameplate => "nameplate",
        }
    }
    pub fn from_id(s: &str) -> Option<Role> {
        ROLES.iter().copied().find(|r| r.id() == s)
    }
    /// The face and weight a role uses unless the project overrides it (400 regular, 500 medium,
    /// 600 semibold; nothing lighter than regular for small CAD text).
    pub fn default_face(self) -> (Face, u16) {
        match self {
            Role::Primary | Role::Dimension => (Face::Sans, 400),
            Role::Condensed | Role::Schedule => (Face::Condensed, 400),
            Role::Monospace => (Face::Mono, 400),
            Role::Heading => (Face::Sans, 600),
            Role::Nameplate => (Face::Sans, 500),
        }
    }
}

/// One coordinated typography system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub sans: &'static str,
    pub condensed: &'static str,
    pub mono: &'static str,
    /// An optional editorial serif of the same family (not used by any role yet).
    pub serif: Option<&'static str>,
    /// The licence the family is published under, as far as it is known. Every file is still
    /// checked against its own licence text before it is packaged.
    pub license: &'static str,
    pub upstream: &'static str,
    /// What must be confirmed before bundling, and any substitution made for a missing face.
    pub note: &'static str,
}

pub const DEFAULT_PRESET: &str = "engineering";

pub const PRESETS: [Preset; 5] = [
    Preset {
        id: "engineering",
        name: "Engineering",
        description: "Technical, balanced, professional: everyday architectural documentation and dense CAD interfaces.",
        sans: "IBM Plex Sans",
        condensed: "IBM Plex Sans Condensed",
        mono: "IBM Plex Mono",
        serif: None,
        license: "SIL OFL 1.1",
        upstream: "https://github.com/IBM/plex",
        note: "The default. Sans, Sans Condensed and Mono belong to one typeface system.",
    },
    Preset {
        id: "editorial",
        name: "Architectural Editorial",
        description: "Refined presentation with strong text hierarchy for sheets and specifications.",
        sans: "Source Sans 3",
        condensed: "Source Sans 3",
        mono: "Source Code Pro",
        serif: Some("Source Serif 4"),
        license: "SIL OFL 1.1",
        upstream: "https://github.com/adobe-fonts",
        note: "The family has no dedicated condensed face, so the Condensed role uses Source Sans 3 at its normal width until a narrower design is approved. Verify the package and licence before bundling.",
    },
    Preset {
        id: "multilingual",
        name: "Multilingual Documentation",
        description: "Broad script coverage for project documents in several languages.",
        sans: "Noto Sans",
        condensed: "Noto Sans Condensed",
        mono: "Noto Sans Mono",
        serif: None,
        license: "SIL OFL 1.1 (verify per file)",
        upstream: "https://github.com/notofonts/notofonts.github.io",
        note: "Bundle only the script subsets the project languages need (for example Devanagari); no single file covers every script. Confirm the condensed width exists in the chosen release.",
    },
    Preset {
        id: "compact",
        name: "Compact Interface",
        description: "Space-efficient typography for toolbars, property panels, schedules and tables.",
        sans: "Roboto",
        condensed: "Roboto Condensed",
        mono: "Roboto Mono",
        serif: None,
        license: "verify in the chosen release",
        upstream: "https://github.com/googlefonts/roboto",
        note: "Verify the licence and the files of the exact release before redistribution.",
    },
    Preset {
        id: "drafting",
        name: "Drafting Utility",
        description: "Clear, utilitarian typography for annotations, coordinates, dimensions and data-heavy drawings.",
        sans: "Fira Sans",
        condensed: "Fira Sans Condensed",
        mono: "Fira Mono",
        serif: None,
        license: "verify in the chosen release",
        upstream: "https://github.com/mozilla/Fira",
        note: "Check the chosen Fira releases and their individual licence files before bundling.",
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

impl Preset {
    pub fn family(&self, face: Face) -> &'static str {
        match face {
            Face::Sans => self.sans,
            Face::Condensed => self.condensed,
            Face::Mono => self.mono,
        }
    }
}

/// Which preset is selected and how roles map to faces. Stored in the workspace and in project
/// templates; a project snapshot copies it so a later change of the application default does not
/// change an existing project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typography {
    pub preset: String,
    /// Per-role overrides of [`Role::default_face`] (face and weight).
    pub overrides: Vec<(Role, Face, u16)>,
}

impl Default for Typography {
    fn default() -> Self {
        Typography { preset: DEFAULT_PRESET.into(), overrides: Vec::new() }
    }
}

/// A role resolved to a font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub role: Role,
    /// The family that will be used (the requested one when installed, else the fallback).
    pub family: String,
    pub weight: u16,
    /// The family the preset asked for.
    pub requested: String,
    /// The requested family was not found, so another was substituted (report it, do not hide it).
    pub substituted: bool,
    /// The weight exists as its own face (always true for Regular). When false the Regular face is
    /// drawn and the role is not at its intended weight.
    pub weight_installed: bool,
}

/// Whether a family can be found (installed, in the application font folder, or registered).
pub fn installed(family: &str) -> bool {
    ttf::find(family).is_some()
}

impl Typography {
    pub fn preset(&self) -> &'static Preset {
        preset(&self.preset).unwrap_or(&PRESETS[0])
    }

    fn face_of(&self, role: Role) -> (Face, u16) {
        self.overrides.iter().find(|(r, _, _)| *r == role).map_or_else(|| role.default_face(), |(_, f, w)| (*f, *w))
    }

    /// The font for `role`. A missing family falls back, in this order, to the preset's other faces
    /// (Sans, then Condensed, then Mono), then to the built-in stroke font name, so the result is the
    /// same on every machine with the same fonts installed.
    pub fn resolve(&self, role: Role) -> Resolved {
        let (face, weight) = self.face_of(role);
        let p = self.preset();
        let requested = p.family(face).to_string();
        let chain = [face, Face::Sans, Face::Condensed, Face::Mono];
        for f in chain {
            let fam = p.family(f);
            if installed(fam) {
                let weight_installed = weight_name(weight).is_none() || find_weighted(fam, weight).is_some();
                return Resolved { role, family: fam.to_string(), weight, substituted: fam != requested, weight_installed, requested };
            }
        }
        Resolved { role, family: crate::BUILTIN_FONT.to_string(), weight, substituted: true, weight_installed: false, requested }
    }

    /// Every role resolved: what the UI and the drawing styles will use.
    pub fn resolve_all(&self) -> Vec<Resolved> {
        ROLES.iter().map(|r| self.resolve(*r)).collect()
    }
}

/// The style name a weight goes by in font file names (`None`: regular or lighter).
pub fn weight_name(weight: u16) -> Option<&'static str> {
    match weight {
        500 => Some("Medium"),
        600 => Some("SemiBold"),
        700.. => Some("Bold"),
        _ => None,
    }
}

/// The face of `family` at `weight`, when that weight is installed as its own face (Medium,
/// SemiBold, Bold). Regular weights are the family itself. A missing weight is `None`: the caller
/// decides whether to use Regular, it is never assumed.
pub fn find_weighted(family: &str, weight: u16) -> Option<ttf::Face> {
    let Some(name) = weight_name(weight) else { return ttf::find(family) };
    [format!("{family} {name}"), format!("{}-{name}", family.replace(' ', "")), format!("{} {name}", family.replace(' ', ""))]
        .iter()
        .find_map(|c| ttf::find(c))
}

/// The family name to put in a text style for `family` at `weight`: `IBM Plex Sans SemiBold` for
/// 600, the plain family for Regular. The style then asks for that face by name; if it is not
/// installed the drawing falls back as any missing font does.
pub fn family_at(family: &str, weight: u16) -> String {
    match weight_name(weight) {
        Some(n) => format!("{family} {n}"),
        None => family.to_string(),
    }
}

/// The text style a role uses in a drawing: `AAD-Primary`, `AAD-Dimension` and so on.
pub fn style_name(role: Role) -> String {
    let id = role.id();
    format!("AAD-{}{}", id.get(..1).unwrap_or("").to_ascii_uppercase(), id.get(1..).unwrap_or(""))
}

/// Characters of `text` that `family` has no glyph for (empty when it covers them all, or when the
/// family is not installed: use [`installed`] to tell those apart).
pub fn missing_glyphs(family: &str, text: &str) -> Vec<char> {
    let Some(face) = ttf::find(family) else { return Vec::new() };
    let mut out: Vec<char> = text.chars().filter(|c| !c.is_whitespace() && !c.is_control() && !face.has_glyph(*c)).collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_presets_each_with_three_faces_and_unique_ids() {
        assert_eq!(PRESETS.len(), 5);
        let ids: std::collections::HashSet<_> = PRESETS.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 5);
        assert!(PRESETS.iter().all(|p| !p.sans.is_empty() && !p.condensed.is_empty() && !p.mono.is_empty()));
        assert_eq!(preset(DEFAULT_PRESET).map(|p| p.sans), Some("IBM Plex Sans"));
        assert!(preset("nope").is_none());
        // The one preset whose Condensed is not a separate design says so.
        assert!(PRESETS.iter().filter(|p| p.sans == p.condensed).all(|p| p.note.contains("no dedicated condensed")));
    }

    #[test]
    fn every_role_maps_to_a_face_and_never_below_regular_weight() {
        for r in ROLES {
            let (_, w) = r.default_face();
            assert!((400..=700).contains(&w), "{r:?}");
            assert_eq!(Role::from_id(r.id()), Some(r));
        }
        assert_eq!(Role::Monospace.default_face().0, Face::Mono);
        assert_eq!(Role::Schedule.default_face().0, Face::Condensed);
    }

    #[test]
    fn a_missing_family_falls_back_deterministically_and_says_so() {
        // None of the preset fonts is registered in a test run, so every role falls through to the
        // built-in font and is reported as substituted.
        let t = Typography::default();
        let r = t.resolve(Role::Monospace);
        assert_eq!(r.requested, "IBM Plex Mono");
        if !installed("IBM Plex Mono") && !installed("IBM Plex Sans") && !installed("IBM Plex Sans Condensed") {
            assert_eq!(r.family, crate::BUILTIN_FONT);
            assert!(r.substituted);
        }
        // An unknown preset id means the default preset, not a failure.
        assert_eq!(Typography { preset: "nope".into(), overrides: vec![] }.preset().id, DEFAULT_PRESET);
        // An override changes the face and weight of one role only.
        let o = Typography { preset: "engineering".into(), overrides: vec![(Role::Heading, Face::Condensed, 700)] };
        assert_eq!(o.resolve(Role::Heading).weight, 700);
        assert_eq!(o.resolve(Role::Primary).weight, 400);
        assert_eq!(o.resolve_all().len(), 7);
    }

    #[test]
    fn glyph_coverage_of_a_missing_family_is_empty_not_an_error() {
        assert!(missing_glyphs("No Such Family 123", "abc").is_empty());
        assert!(!installed("No Such Family 123"));
    }
}
