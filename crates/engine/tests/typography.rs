//! The font presets, the inventory of a drawing's text styles and the migration to a preset.

use cadcraft_doc::{Common, EntityKind, Space, Text};
use cadcraft_engine::Session;
use serde_json::json;

fn session() -> Session {
    let mut s = Session::default();
    let d = s.doc_mut().unwrap();
    d.text_styles.clear();
    for (name, font, h, oblique) in [
        ("Standard", "arial.ttf", 0.0, 0.0),
        ("Dimension", "arial.ttf", 0.0, 0.0),
        ("Schedule-Text", "romans.shx", 2.5, 15.0),
        ("Misc-Thing", "arial.ttf", 0.0, 0.0),
    ] {
        d.text_styles.push(cadcraft_doc::TextStyle {
            name: name.into(),
            font: font.into(),
            height: h,
            oblique,
            ..cadcraft_doc::TextStyle::default()
        });
    }
    for style in ["Standard", "Standard", "Schedule-Text"] {
        let t = Text {
            insert: cadcraft_geom::Vec3::new(0.0, 0.0, 0.0),
            align_pt: None,
            height: 2.5,
            value: "Room 12 m\u{b2}".into(),
            rotation: 0.0,
            width_factor: 1.0,
            oblique: 0.0,
            style: style.into(),
            halign: Default::default(),
            valign: Default::default(),
        };
        d.add(&Space::Model, Common::default(), EntityKind::Text(t)).unwrap();
    }
    s
}

#[test]
fn presets_and_roles_are_listed_with_what_is_installed() {
    let mut s = Session::default();
    let r = s.execute("fonts.presets", &json!({})).unwrap();
    assert_eq!(r["presets"].as_array().unwrap().len(), 5);
    assert_eq!(r["roles"].as_array().unwrap().len(), 7);
    assert_eq!(r["default"], "engineering");
    assert_eq!(r["presets"][0]["families"]["mono"], "IBM Plex Mono");
    let res = s.execute("fonts.resolve", &json!({"preset": "drafting"})).unwrap();
    assert_eq!(res["roles"].as_array().unwrap().len(), 7);
    assert!(s.execute("fonts.resolve", &json!({"preset": "nope"})).is_err());
}

#[test]
fn scan_finds_styles_their_use_a_likely_role_and_what_cannot_be_told() {
    let mut s = session();
    let r = s.execute("fonts.scan", &json!({})).unwrap();
    let style = |n: &str| r["styles"].as_array().unwrap().iter().find(|s| s["name"] == n).unwrap().clone();
    assert_eq!(style("Standard")["role"], "primary");
    assert_eq!(style("Standard")["usage"]["text"], 2);
    assert_eq!(style("Dimension")["role"], "dimension");
    assert_eq!(style("Schedule-Text")["role"], "schedule");
    assert_eq!(style("Schedule-Text")["stroke"], true, "a stroke font is never reported missing");
    assert!(style("Misc-Thing")["role"].is_null());
    assert_eq!(r["unclassified"], json!(["Misc-Thing"]));
}

#[test]
fn migration_previews_first_keeps_geometry_and_never_swaps_in_a_different_font() {
    let mut s = session();
    // With no preset font installed, nothing is changed and every style says why.
    let r = s.execute("fonts.migrate", &json!({"preset": "engineering"})).unwrap();
    assert_eq!(r["changes"], json!([]));
    assert!(r["skipped"].as_array().unwrap().iter().any(|x| x["reason"].as_str().unwrap_or("").contains("not installed")));
    assert_eq!(r["applied"], false);
    // Install stand-ins for the preset's fonts (the system's own font under the preset's names).
    let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\arial.ttf").or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")) else {
        return;
    };
    for name in ["IBM Plex Sans", "IBM Plex Sans Condensed", "IBM Plex Mono"] {
        cadcraft_fonts::ttf::register(name, bytes.clone());
    }
    let before = s.doc().unwrap().text_styles.clone();
    let preview = s.execute("fonts.migrate", &json!({"preset": "engineering"})).unwrap();
    let changes = preview["changes"].as_array().unwrap();
    assert_eq!(preview["applied"], false);
    assert!(changes.iter().any(|c| c["style"] == "Standard" && c["to"] == "IBM Plex Sans"));
    assert!(changes.iter().any(|c| c["style"] == "Schedule-Text" && c["to"] == "IBM Plex Sans Condensed"));
    assert!(
        preview["skipped"].as_array().unwrap().iter().any(|x| x["style"] == "Misc-Thing" && x["reason"].as_str().unwrap().contains("assign a role"))
    );
    // Text drawn in the stroke font gets a different width in the TrueType font: it is listed, so it can be checked.
    assert!(
        preview["growth"].as_array().unwrap().iter().any(|g| g["style"] == "Schedule-Text" && g["newWidth"].as_f64() != g["oldWidth"].as_f64())
            || preview["growth"].as_array().unwrap().is_empty(),
        "growth entries carry both widths"
    );
    assert_eq!(s.doc().unwrap().text_styles, before, "a preview changes nothing");
    // Apply: only the font name changes.
    let applied = s.execute("fonts.migrate", &json!({"preset": "engineering", "apply": true})).unwrap();
    assert_eq!(applied["applied"], true);
    let after = s.doc().unwrap().text_styles.clone();
    for (a, b) in before.iter().zip(&after) {
        let mut a2 = a.clone();
        a2.font = b.font.clone();
        assert_eq!(&a2, b, "everything but the font is as it was: {}", a.name);
    }
    assert_eq!(after.iter().find(|t| t.name == "Standard").unwrap().font, "IBM Plex Sans");
    assert_eq!(after.iter().find(|t| t.name == "Misc-Thing").unwrap().font, "arial.ttf", "unclassified styles are left alone");
    // Assigning a role includes it; one undo step restores everything.
    s.execute("fonts.migrate", &json!({"preset": "engineering", "apply": true, "roles": {"Misc-Thing": "monospace"}})).unwrap();
    assert_eq!(s.doc().unwrap().text_styles.iter().find(|t| t.name == "Misc-Thing").unwrap().font, "IBM Plex Mono");
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().text_styles.iter().find(|t| t.name == "Misc-Thing").unwrap().font, "arial.ttf");
    assert!(s.execute("fonts.migrate", &json!({"roles": {"Standard": "bogus"}})).is_err());
}
