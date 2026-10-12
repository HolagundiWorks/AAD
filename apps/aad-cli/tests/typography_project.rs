//! Fonts across a generated project: new drawings carry the preset's styles, a project-wide migration
//! backs up and reports, DXF reopen keeps the fonts, and a rollback restores what was there.

use std::path::PathBuf;
use std::sync::Once;

use aad_engine::Session;
use serde_json::{Value, json};

fn install_io() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        aad_engine::cmd::file::set_io(aad_engine::cmd::file::IoHooks {
            read: |b, name| aad_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| aad_io::write(d, name).map_err(|e| e.to_string()),
            plot: Some(|d, space, opts| aad_io::plot(d, space, opts).map_err(|e| e.to_string())),
        });
    });
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("aad-typo-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    d
}

fn config(preset: &str) -> Value {
    json!({
        "schema_version": 1,
        "project": {"id": "prj-t", "code": "PRJ-T", "name": "Typography Test"},
        "floors": [{"id": "f0", "name": "Ground", "code": "GROUND", "sequence": 0, "elevation": 0.0, "floor_to_floor": 3000.0}],
        "drawing_sets": [{"id": "s-ar", "code": "AR", "name": "Architectural", "enabled": true}],
        "typography": {"preset": preset}
    })
}

fn style_font(s: &mut Session, file: &std::path::Path, style: &str) -> String {
    let i = s.execute("open", &json!({"path": file.to_str().unwrap()})).unwrap()["index"].as_u64().unwrap();
    let font = s.doc().unwrap().text_styles.iter().find(|t| t.name == style).map(|t| t.font.clone()).unwrap_or_default();
    s.execute("close", &json!({"index": i})).unwrap();
    font
}

#[test]
fn a_project_is_created_with_the_preset_and_migrated_backed_up_and_rolled_back() {
    install_io();
    let mut s = Session::new();
    let root = temp("project");
    let r = s.execute("project.generate", &json!({"config": config("drafting"), "root": root.to_str().unwrap()})).unwrap();
    assert_eq!(r["status"], "ready", "{r}");
    let ground = root.join("drawings/Architectural/PRJ-T_AR_GROUND.dxf");
    // New drawings name the preset's fonts, with the weight in the name for headings.
    assert_eq!(style_font(&mut s, &ground, "AAD-Primary"), "Fira Sans");
    assert_eq!(style_font(&mut s, &ground, "AAD-Monospace"), "Fira Mono");
    assert_eq!(style_font(&mut s, &ground, "AAD-Heading"), "Fira Sans SemiBold");
    assert_eq!(style_font(&mut s, &ground, "Standard"), "Fira Sans", "the Standard style follows the Primary role");
    // An unknown preset in a configuration is refused before anything is written.
    let bad = s.execute("project.plan", &json!({"config": config("comic")})).unwrap();
    assert_eq!(bad["valid"], false);
    assert_eq!(bad["issues"][0]["path"], "typography.preset");

    // Stand-ins for the Engineering preset's fonts (the system font under their names).
    let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\arial.ttf").or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")) else {
        return;
    };
    for name in ["IBM Plex Sans", "IBM Plex Sans Condensed", "IBM Plex Mono", "IBM Plex Sans SemiBold", "IBM Plex Sans Medium"] {
        aad_fonts::ttf::register(name, bytes.clone());
    }
    let before = std::fs::read(&ground).unwrap();
    let rootp = root.to_str().unwrap();
    // Preview over the project changes nothing on disk.
    let p = s.execute("fonts.migrate", &json!({"preset": "engineering", "root": rootp})).unwrap();
    assert_eq!(p["applied"], false);
    assert_eq!(std::fs::read(&ground).unwrap(), before);
    assert!(!root.join("backups/fonts-001").exists());
    // Apply: every drawing is backed up, saved, and reported.
    let a = s.execute("fonts.migrate", &json!({"preset": "engineering", "root": rootp, "apply": true})).unwrap();
    assert_eq!(a["applied"], true, "{a}");
    let id = a["migrationId"].as_str().unwrap().to_string();
    assert!(root.join("backups").join(&id).join("report.json").is_file());
    assert_eq!(
        std::fs::read(root.join("backups").join(&id).join("drawings/Architectural/PRJ-T_AR_GROUND.dxf")).unwrap(),
        before,
        "the backup is the drawing as it was"
    );
    // The DXF reopens with the new fonts, and nothing but the font names changed.
    assert_eq!(style_font(&mut s, &ground, "AAD-Primary"), "IBM Plex Sans");
    assert_eq!(style_font(&mut s, &ground, "AAD-Heading"), "IBM Plex Sans SemiBold");
    assert_eq!(style_font(&mut s, &ground, "AAD-Monospace"), "IBM Plex Mono");
    // Roll back: the drawing is byte for byte as it was before.
    let rb = s.execute("fonts.rollback", &json!({"root": rootp, "id": id})).unwrap();
    assert!(!rb["restored"].as_array().unwrap().is_empty(), "{rb}");
    assert_eq!(std::fs::read(&ground).unwrap(), before);
    assert_eq!(style_font(&mut s, &ground, "AAD-Primary"), "Fira Sans");
    // A drawing edited after a migration is left alone by its rollback.
    let a2 = s.execute("fonts.migrate", &json!({"preset": "engineering", "root": rootp, "apply": true})).unwrap();
    let id2 = a2["migrationId"].as_str().unwrap().to_string();
    s.execute("open", &json!({"path": ground.to_str().unwrap()})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [100, 0]]})).unwrap();
    s.execute("qsave", &json!({})).unwrap();
    let i = s.active;
    s.execute("close", &json!({"index": i})).unwrap();
    let edited = std::fs::read(&ground).unwrap();
    let rb2 = s.execute("fonts.rollback", &json!({"root": rootp, "id": id2})).unwrap();
    assert!(rb2["left"].as_array().unwrap().iter().any(|l| l["reason"].as_str().unwrap().contains("edited since")), "{rb2}");
    assert_eq!(std::fs::read(&ground).unwrap(), edited);
    assert!(s.execute("fonts.rollback", &json!({"root": rootp, "id": "../x"})).is_err());
}
