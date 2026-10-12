#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::company::{self, CompanyProfile};
use super::*;

fn floor(id: &str, name: &str, code: &str, seq: u32, elevation: Option<f64>, h: Option<f64>) -> Floor {
    Floor { id: id.into(), name: name.into(), code: code.into(), sequence: seq, elevation, floor_to_floor: h, ..Floor::default() }
}

fn set(id: &str, code: &str, name: &str, model: bool) -> DrawingSet {
    DrawingSet { id: id.into(), code: code.into(), name: name.into(), enabled: true, has_model: model, ..DrawingSet::default() }
}

/// A small valid project: three floors with different heights and two sets.
fn sample() -> ProjectConfig {
    ProjectConfig {
        project: ProjectInfo { id: "prj-1".into(), code: "PRJ-001".into(), name: "Residential Building".into(), ..ProjectInfo::default() },
        floors: vec![
            floor("f0", "Ground Floor", "GROUND", 0, Some(0.0), Some(3300.0)),
            floor("f1", "First Floor", "FIRST", 1, None, Some(3000.0)),
            floor("f2", "Terrace", "TERRACE", 2, None, None),
        ],
        drawing_sets: vec![set("s-ar", "AR", "Architectural", true), set("s-st", "ST", "Structural", false)],
        xref_rules: vec![
            // Architectural floor drawings reference the architectural model.
            XrefRule {
                host: Selector { kind: DrawingKind::Floor, set: "AR".into(), same_floor: false },
                target: Selector { kind: DrawingKind::Model, set: "AR".into(), same_floor: false },
                attachment: Attachment::Overlay,
            },
            // The master references every architectural floor drawing.
            XrefRule {
                host: Selector { kind: DrawingKind::Master, ..Selector::default() },
                target: Selector { kind: DrawingKind::Floor, set: "AR".into(), same_floor: false },
                attachment: Attachment::Attach,
            },
        ],
        ..ProjectConfig::default()
    }
}

fn paths(p: &Plan) -> Vec<&str> {
    p.drawings.iter().map(|d| d.rel_path.as_str()).collect()
}

#[test]
fn a_valid_configuration_plans_folders_drawings_and_xrefs_from_the_data() {
    let p = plan(&sample()).expect("valid");
    assert_eq!(
        paths(&p),
        [
            "drawings/00_Master/PRJ-001_MASTER.dxf",
            "drawings/Architectural/PRJ-001_AR_MODEL.dxf",
            "drawings/Architectural/PRJ-001_AR_GROUND.dxf",
            "drawings/Architectural/PRJ-001_AR_FIRST.dxf",
            "drawings/Architectural/PRJ-001_AR_TERRACE.dxf",
            "drawings/Structural/PRJ-001_ST_GROUND.dxf",
            "drawings/Structural/PRJ-001_ST_FIRST.dxf",
            "drawings/Structural/PRJ-001_ST_TERRACE.dxf",
        ]
    );
    assert!(p.folders.contains(&"drawings/Structural".to_string()) && p.folders.contains(&"backups".to_string()));
    assert_eq!(p.files, ["project.json"], "no schedules configured");
    // 3 floor drawings -> model, and the master -> 3 floor drawings.
    assert_eq!(p.xrefs.len(), 6);
    let ground = p.drawings.iter().find(|d| d.rel_path.ends_with("AR_GROUND.dxf")).unwrap();
    let x = p.xrefs.iter().find(|x| x.host_id == ground.id).unwrap();
    assert_eq!(x.relative_path, "PRJ-001_AR_MODEL.dxf", "same folder");
    let master = p.drawings.iter().find(|d| d.kind == DrawingKind::Master).unwrap();
    let m = p.xrefs.iter().find(|x| x.host_id == master.id).unwrap();
    assert!(m.relative_path.starts_with("../Architectural/"), "{}", m.relative_path);
    assert_eq!(ground.number, "PRJ-001-AR-002", "the model is 001, the floors follow");
    assert_eq!(ground.title, "Ground Floor Architectural");
}

#[test]
fn elevations_are_derived_from_heights_and_missing_ones_are_reported_not_defaulted() {
    let p = plan(&sample()).unwrap();
    assert_eq!(p.elevations, [("f0".to_string(), 0.0), ("f1".to_string(), 3300.0), ("f2".to_string(), 6300.0)]);
    let mut c = sample();
    c.floors[0].floor_to_floor = None;
    let e = c.validate();
    assert!(e.iter().any(|i| i.path == "floors[1].elevation" && i.message.contains("no floor-to-floor height")), "{e:?}");
    // A manual elevation is kept as given, and one that is not above the floor below is an error.
    let mut c = sample();
    c.floors[1].elevation = Some(3500.0);
    assert_eq!(plan(&c).unwrap().elevations[1].1, 3500.0);
    c.floors[1].elevation = Some(-10.0);
    assert!(c.validate().iter().any(|i| i.message.contains("not above")));
    // The lowest floor needs a datum elevation.
    let mut c = sample();
    c.floors[0].elevation = None;
    assert!(c.validate().iter().any(|i| i.path == "floors[0].elevation"));
}

#[test]
fn the_matrix_leaves_floors_out_and_ids_are_stable_across_renames_and_reruns() {
    let mut c = sample();
    c.matrix.push(MatrixCell { floor_id: "f2".into(), set_id: "s-st".into(), create: false });
    let a = plan(&c).unwrap();
    assert_eq!(a.drawings.len(), 7, "no structural terrace drawing");
    assert_eq!(a, plan(&c).unwrap(), "the same configuration gives the same plan");
    let id_of = |p: &Plan| p.drawings.iter().find(|d| d.rel_path.contains("AR_FIRST")).unwrap().id.clone();
    let before = id_of(&a);
    c.floors[1].name = "Level 1".into();
    c.drawing_sets[0].name = "Architecture".into();
    c.drawing_sets[0].folder = "AR-Plans".into();
    let b = plan(&c).unwrap();
    assert_eq!(id_of(&b), before, "renaming and moving a drawing keeps its identity");
    assert!(paths(&b).iter().any(|p| p.starts_with("drawings/AR-Plans/")));
    // A disabled set creates nothing.
    c.drawing_sets[1].enabled = false;
    assert!(!paths(&plan(&c).unwrap()).iter().any(|p| p.contains("Structural")));
}

#[test]
fn bad_input_is_reported_with_where_and_nothing_panics() {
    let mut c = sample();
    c.project.code = "PRJ 001/..".into();
    c.floors[1].code = "GROUND".into();
    c.floors[2].name = "ground floor".into();
    c.drawing_sets[0].folder = "../escape".into();
    c.naming_rules.drawing_pattern = "{project}_{nope}".into();
    c.naming_rules.file_extension = "exe".into();
    c.xref_rules[0].target.set = "ZZ".into();
    let e = c.validate();
    for want in [
        "project.code",
        "floors[1].code",
        "floors[2].name",
        "drawing_sets[0].folder",
        "naming_rules.drawing_pattern",
        "naming_rules.file_extension",
        "xref_rules[0].target.set",
    ] {
        assert!(e.iter().any(|i| i.path == want), "{want} missing in {e:?}");
    }
    assert!(plan(&c).is_err());
    // Hostile sizes and values.
    let mut c = sample();
    c.floors = (0..500).map(|i| floor(&format!("f{i}"), &format!("F{i}"), &format!("C{i}"), i, Some(f64::from(i)), Some(1.0))).collect();
    assert!(c.validate().iter().any(|i| i.path == "floors"));
    let mut c = sample();
    c.floors[0].elevation = Some(f64::NAN);
    c.floors[0].floor_to_floor = Some(f64::INFINITY);
    assert!(!c.validate().is_empty());
    assert!(ProjectConfig::from_json("{ not json").is_err());
    assert!(ProjectConfig::from_json(&"x".repeat(MAX_MANIFEST_BYTES + 1)).is_err());
    assert!(ProjectConfig::from_json(r#"{"schema_version": 99}"#).unwrap_err().contains("update the application"));
}

#[test]
fn drawings_that_would_share_a_file_and_xref_cycles_are_refused() {
    let mut c = sample();
    c.naming_rules.drawing_pattern = "{project}_{set}".into();
    assert!(c.validate().iter().any(|i| i.message.contains("is the file name of both")));
    let mut c = sample();
    // The architectural model references the master, which references the floors, which reference the model.
    c.xref_rules.push(XrefRule {
        host: Selector { kind: DrawingKind::Model, set: "AR".into(), same_floor: false },
        target: Selector { kind: DrawingKind::Master, ..Selector::default() },
        attachment: Attachment::Attach,
    });
    let e = c.validate();
    assert!(e.iter().any(|i| i.message.contains("in a circle")), "{e:?}");
    // A rule that matches a drawing with itself adds no edge.
    let mut c = sample();
    c.xref_rules = vec![XrefRule {
        host: Selector { kind: DrawingKind::Floor, set: "AR".into(), same_floor: false },
        target: Selector { kind: DrawingKind::Floor, set: "AR".into(), same_floor: true },
        attachment: Attachment::Attach,
    }];
    assert!(plan(&c).unwrap().xrefs.is_empty());
}

#[test]
fn relative_paths_climb_out_of_the_host_folder() {
    assert_eq!(relative_path("drawings/A/x.dxf", "drawings/A/y.dxf"), "y.dxf");
    assert_eq!(relative_path("drawings/A/x.dxf", "drawings/B/y.dxf"), "../B/y.dxf");
    assert_eq!(relative_path("x.dxf", "drawings/B/y.dxf"), "drawings/B/y.dxf");
    assert_eq!(relative_path("a/b/c/x.dxf", "y.dxf"), "../../../y.dxf");
}

#[test]
fn the_manifest_round_trips_and_missing_fields_take_defaults() {
    let c = sample();
    assert_eq!(ProjectConfig::from_json(&c.to_json()).unwrap(), c);
    let min = ProjectConfig::from_json(r#"{"schema_version": 1, "project": {"code": "A"}}"#).unwrap();
    assert!(min.master_drawing && min.naming_rules.file_extension == "dxf" && min.floors.is_empty());
}

#[test]
fn preflight_reports_what_exists_and_changes_nothing() {
    let dir = std::env::temp_dir().join(format!("aad-preflight-{}", new_id("t")));
    let p = plan(&sample()).unwrap();
    assert!(!preflight(&dir, &p).root_exists);
    std::fs::create_dir_all(dir.join("drawings/00_Master")).unwrap();
    std::fs::write(dir.join("drawings/00_Master/PRJ-001_MASTER.dxf"), b"mine").unwrap();
    let r = preflight(&dir, &p);
    assert!(r.root_exists && !r.root_is_empty);
    assert_eq!(r.existing_files, ["drawings/00_Master/PRJ-001_MASTER.dxf"]);
    assert_eq!(std::fs::read(dir.join("drawings/00_Master/PRJ-001_MASTER.dxf")).unwrap(), b"mine");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ids_are_distinct_and_codes_are_checked() {
    let ids: std::collections::HashSet<String> = (0..1000).map(|_| new_id("x")).collect();
    assert_eq!(ids.len(), 1000);
    assert!(ids.iter().all(|i| is_code(i)));
    assert!(!is_code("") && !is_code("a b") && !is_code("../x") && !is_code(&"a".repeat(49)));
}

fn temp_base() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("aad-company-{}", new_id("t")))
}

#[test]
fn a_company_profile_is_saved_loaded_listed_and_found_by_id_not_name() {
    let base = temp_base();
    let mut a = CompanyProfile::new("Example Architecture Studio");
    a.address.city = "Bengaluru".into();
    a.contact.email = "studio@example.com".into();
    let b = CompanyProfile::new("Example Architecture Studio");
    assert_ne!(a.id, b.id, "two firms may share a name");
    company::save(&base, &a).unwrap();
    company::save(&base, &b).unwrap();
    assert_eq!(company::load(&base, &a.id).unwrap(), a);
    assert_eq!(company::list(&base).len(), 2);
    assert!(company::load(&base, "company-missing").unwrap_err().contains("not found"));
    assert!(company::load(&base, "../etc").is_err(), "an id cannot climb out of the store");
    // A corrupt file is an error to show, not a crash, and does not hide the other profile.
    std::fs::write(base.join("companies").join(&a.id).join("company.json"), "{ broken").unwrap();
    assert!(company::load(&base, &a.id).is_err());
    assert_eq!(company::list(&base).len(), 1);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn profile_validation_logo_handling_and_nameplate_lines_skip_empty_fields() {
    let base = temp_base();
    let mut p = CompanyProfile::new("Studio");
    p.contact.email = "not an email".into();
    assert!(p.validate().iter().any(|i| i.path == "contact.email"));
    p.contact.email = String::new();
    assert_eq!(p.nameplate_lines(), ["Studio"], "nothing else is printed");
    p.address.line1 = "12 Main Road".into();
    p.address.city = "Pune".into();
    p.address.postal_code = "411001".into();
    p.contact.phone = "+91 20 0000 0000".into();
    p.architect.name = "A. Architect".into();
    p.architect.registration_number = "CA/1234".into();
    assert_eq!(p.nameplate_lines(), ["Studio", "12 Main Road", "Pune, 411001", "+91 20 0000 0000", "A. Architect · CA/1234"]);
    // Logo: only images, copied into the profile, recorded relative to it.
    std::fs::create_dir_all(&base).unwrap();
    let png = base.join("mark.png");
    std::fs::write(&png, [0x89, b'P', b'N', b'G']).unwrap();
    company::set_logo(&base, &mut p, &png).unwrap();
    assert_eq!(p.logo.relative_path, "assets/logo.png");
    assert!(company::profile_dir(&base, &p.id).unwrap().join("assets/logo.png").is_file());
    let txt = base.join("notes.txt");
    std::fs::write(&txt, "x").unwrap();
    assert!(company::set_logo(&base, &mut p, &txt).is_err());
    p.logo.relative_path = "../../outside.png".into();
    assert!(p.validate().iter().any(|i| i.path == "logo.relative_path"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_recorded_logo_must_exist_and_the_nameplate_text_does_not_need_it() {
    let base = temp_base();
    let mut p = CompanyProfile::new("Studio");
    p.architect.name = "A. Architect".into();
    company::save(&base, &p).unwrap();
    assert!(company::validate_assets(&base, &p).is_empty(), "no logo recorded: nothing to check");
    p.logo.relative_path = "assets/logo.png".into();
    let issues = company::validate_assets(&base, &p);
    assert!(issues.iter().any(|i| i.path == "logo.relative_path" && i.message.contains("not in the profile folder")), "{issues:?}");
    // The nameplate lines are text only: complete with or without a logo.
    assert_eq!(p.nameplate_lines(), ["Studio", "A. Architect"]);
    let png = base.join("mark.png");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(&png, [0x89, b'P', b'N', b'G']).unwrap();
    company::set_logo(&base, &mut p, &png).unwrap();
    assert!(company::validate_assets(&base, &p).is_empty());
    assert_eq!(p.nameplate_lines(), ["Studio", "A. Architect"], "storing the logo changes no text line");
    let _ = std::fs::remove_dir_all(&base);
}
