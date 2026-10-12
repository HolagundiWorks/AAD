//! Generating a project from a configuration on real files: the drawings, the Xrefs, the manifest,
//! never overwriting, interrupted runs and recovery.

use std::path::{Path, PathBuf};
use std::sync::Once;

use cadcraft_engine::Session;
use serde_json::{Value, json};

fn install_io() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
            read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
            write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
            plot: Some(|d, space, opts| cadcraft_io::plot(d, space, opts).map_err(|e| e.to_string())),
        });
    });
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("aad-gen-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    d
}

fn session() -> Session {
    install_io();
    Session::new()
}

fn config() -> Value {
    json!({
        "schema_version": 1,
        "project": {"id": "prj-1", "code": "PRJ-001", "name": "Residential Building", "client": {"name": "S. Rao"}},
        "floors": [
            {"id": "f0", "name": "Ground Floor", "code": "GROUND", "sequence": 0, "elevation": 0.0, "floor_to_floor": 3300.0},
            {"id": "f1", "name": "First Floor", "code": "FIRST", "sequence": 1, "floor_to_floor": 3000.0}
        ],
        "drawing_sets": [
            {"id": "s-ar", "code": "AR", "name": "Architectural", "enabled": true, "has_model": true},
            {"id": "s-st", "code": "ST", "name": "Structural", "enabled": true}
        ],
        "xref_rules": [
            {"host": {"kind": "floor", "set": "AR"}, "target": {"kind": "model", "set": "AR"}, "attachment": "attach"},
            {"host": {"kind": "master"}, "target": {"kind": "floor", "set": "AR"}, "attachment": "attach"},
            {"host": {"kind": "floor", "set": "ST"}, "target": {"kind": "floor", "set": "AR", "same_floor": true}, "attachment": "overlay"}
        ]
    })
}

fn generate_in(s: &mut Session, root: &Path, extra: Value) -> Value {
    let mut p = json!({"config": config(), "root": root.to_str().unwrap()});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    s.execute("project.generate", &p).unwrap()
}

fn xref_names(s: &mut Session, file: &Path) -> Vec<(String, String)> {
    let r = s.execute("open", &json!({"path": file.to_str().unwrap()})).unwrap();
    let i = r["index"].as_u64().unwrap();
    let list = s.execute("xref.list", &json!({})).unwrap();
    s.execute("close", &json!({"index": i})).unwrap();
    list["xrefs"].as_array().unwrap().iter().map(|e| (e["name"].as_str().unwrap().to_string(), e["status"].as_str().unwrap().to_string())).collect()
}

#[test]
fn generating_creates_every_drawing_links_the_xrefs_and_ends_ready() {
    let root = temp("ready");
    let mut s = session();
    let r = generate_in(&mut s, &root, json!({}));
    assert_eq!(r["status"], "ready", "{r}");
    assert_eq!(r["issues"], json!([]));
    // master + AR model + 2 AR floors + 2 ST floors
    assert_eq!(r["created"].as_array().unwrap().len(), 6);
    assert_eq!(r["xrefs_attached"], 4, "two floors -> model, master -> two floors");
    assert_eq!(r["xrefs_recorded"], 2, "the overlay edges are recorded, not written into the files");
    for f in [
        "drawings/00_Master/PRJ-001_MASTER.dxf",
        "drawings/Architectural/PRJ-001_AR_MODEL.dxf",
        "drawings/Structural/PRJ-001_ST_FIRST.dxf",
        "project.json",
    ] {
        assert!(root.join(f).is_file(), "{f}");
    }
    assert!(!root.join("generation.log.json").exists(), "the log goes away once the project is ready");
    // The manifest records the result; reopening the project reads it back.
    let m = cadcraft_engine::projectplan::ProjectConfig::from_json(&std::fs::read_to_string(root.join("project.json")).unwrap()).unwrap();
    assert_eq!(m.generation.status, cadcraft_engine::projectplan::Status::Ready);
    assert_eq!((m.generated.drawings.len(), m.generated.xrefs.len()), (6, 6));
    // The saved files hold the Xrefs and they resolve from the new location too (the folder moves).
    let master = root.join("drawings/00_Master/PRJ-001_MASTER.dxf");
    let names = xref_names(&mut s, &master);
    assert_eq!(names.len(), 2);
    assert!(names.iter().all(|(_, st)| st == "found"), "{names:?}");
    let moved = temp("ready-moved");
    std::fs::create_dir_all(&moved).unwrap();
    std::fs::rename(&root, moved.join("p")).unwrap();
    let names = xref_names(&mut s, &moved.join("p/drawings/00_Master/PRJ-001_MASTER.dxf"));
    assert!(names.iter().all(|(_, st)| st == "found"), "relative paths survive a move: {names:?}");
    // A generated drawing knows its identity.
    let ground = moved.join("p/drawings/Architectural/PRJ-001_AR_GROUND.dxf");
    let i = s.execute("open", &json!({"path": ground.to_str().unwrap()})).unwrap()["index"].as_u64().unwrap();
    let meta = s.doc().unwrap().project.get("aadDrawing").cloned().unwrap();
    assert!(meta.contains("PRJ-001-AR-002") && meta.contains("Ground Floor"), "{meta}");
    s.execute("close", &json!({"index": i})).unwrap();
}

#[test]
fn an_invalid_configuration_or_busy_folder_writes_nothing() {
    let root = temp("invalid");
    let mut s = session();
    let mut c = config();
    c["project"]["code"] = json!("bad code");
    let r = s.execute("project.generate", &json!({"config": c, "root": root.to_str().unwrap()})).unwrap();
    assert_eq!(r["status"], "draft");
    assert!(!root.exists(), "nothing was created");
    // A folder holding other files is refused unless allowed.
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("notes.txt"), "mine").unwrap();
    let r = generate_in(&mut s, &root, json!({}));
    assert_eq!(r["status"], "draft");
    assert!(r["issues"][0]["message"].as_str().unwrap().contains("not empty"));
    assert!(!root.join("project.json").exists());
    let r = generate_in(&mut s, &root, json!({"allowNonEmpty": true}));
    assert_eq!(r["status"], "ready", "{r}");
    assert_eq!(std::fs::read_to_string(root.join("notes.txt")).unwrap(), "mine");
    // DWG is refused (this build writes DXF).
    let mut c = config();
    c["naming_rules"] = json!({"file_extension": "dwg"});
    let other = temp("dwg");
    let r = s.execute("project.generate", &json!({"config": c, "root": other.to_str().unwrap()})).unwrap();
    assert_eq!(r["status"], "draft");
}

#[test]
fn regenerating_never_overwrites_an_edited_drawing_and_does_not_link_into_it() {
    let root = temp("regen");
    let mut s = session();
    assert_eq!(generate_in(&mut s, &root, json!({}))["status"], "ready");
    let edited = root.join("drawings/Architectural/PRJ-001_AR_FIRST.dxf");
    // The user draws in the first floor plan.
    s.execute("open", &json!({"path": edited.to_str().unwrap()})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [5000, 0]]})).unwrap();
    s.execute("qsave", &json!({})).unwrap();
    let i = s.active;
    s.execute("close", &json!({"index": i})).unwrap();
    let before = std::fs::read(&edited).unwrap();
    // Remove another drawing, then generate again into the same project.
    std::fs::remove_file(root.join("drawings/Structural/PRJ-001_ST_FIRST.dxf")).unwrap();
    let r = generate_in(&mut s, &root, json!({}));
    assert_eq!(r["status"], "ready", "{r}");
    assert_eq!(r["created"], json!(["drawings/Structural/PRJ-001_ST_FIRST.dxf"]), "only the missing drawing is made");
    assert_eq!(r["kept"].as_array().unwrap().len(), 5);
    assert_eq!(std::fs::read(&edited).unwrap(), before, "the edited drawing is byte for byte as the user left it");
    // A different project in the same folder is refused.
    let mut c = config();
    c["project"]["id"] = json!("prj-other");
    let r = s.execute("project.generate", &json!({"config": c, "root": root.to_str().unwrap()})).unwrap();
    assert!(r["issues"][0]["message"].as_str().unwrap().contains("different project"));
}

#[test]
fn an_interrupted_run_is_found_and_can_be_resumed_or_cleaned_up() {
    // Simulate a crash: the run stopped after the master was written, with the next file half written.
    let root = temp("crash");
    let mut s = session();
    assert_eq!(generate_in(&mut s, &root, json!({}))["status"], "ready");
    let master = root.join("drawings/00_Master/PRJ-001_MASTER.dxf");
    let model_rel = "drawings/Architectural/PRJ-001_AR_MODEL.dxf";
    let hash = |p: &Path| std::fs::read(p).unwrap().iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, x| (h ^ u64::from(*x)).wrapping_mul(0x0100_0000_01b3));
    let plan = s.execute("project.plan", &json!({"config": config()})).unwrap();
    let id_of =
        |rel: &str| plan["plan"]["drawings"].as_array().unwrap().iter().find(|d| d["rel_path"] == rel).unwrap()["id"].as_str().unwrap().to_string();
    let master_rel = "drawings/00_Master/PRJ-001_MASTER.dxf";
    let log = json!({
        "project_id": "prj-1", "stage": "generating", "planned": 6,
        "created": [{"id": id_of(master_rel), "rel_path": master_rel, "hash": hash(&master)}],
        "in_progress": id_of(model_rel),
    });
    std::fs::write(root.join("generation.log.json"), log.to_string()).unwrap();
    std::fs::write(root.join(model_rel), "half a dra").unwrap();
    // A user file the log knows nothing about must survive everything.
    std::fs::write(root.join("drawings/Architectural/PRJ-001_AR_GROUND.dxf.bak"), "keep me").unwrap();
    let r = s.execute("project.recover", &json!({"root": root.to_str().unwrap()})).unwrap();
    assert_eq!(r["recovery"]["stage"], "generating");
    assert_eq!(r["recovery"]["unchanged"], json!([master_rel]));
    // Without resume, generating refuses and points at recovery.
    let refused = generate_in(&mut s, &root, json!({}));
    assert!(refused["issues"][0]["message"].as_str().unwrap().contains("interrupted"));
    // Cleanup removes only the recorded, unchanged master; the half file and the user file stay.
    let c = s.execute("project.recover", &json!({"root": root.to_str().unwrap(), "action": "cleanup"})).unwrap();
    assert_eq!(c["removed"], json!([master_rel]));
    assert!(!master.exists());
    assert!(root.join(model_rel).exists() && root.join("drawings/Architectural/PRJ-001_AR_GROUND.dxf.bak").exists());
    assert!(!root.join("generation.log.json").exists());
    // Resume path: rebuild the log, then resume with the saved configuration.
    std::fs::write(
        root.join("generation.log.json"),
        json!({"project_id": "prj-1", "stage": "generating", "planned": 6, "created": [], "in_progress": id_of(model_rel)}).to_string(),
    )
    .unwrap();
    let r = s.execute("project.generate", &json!({"root": root.to_str().unwrap(), "resume": true})).unwrap();
    assert_eq!(r["status"], "ready", "{r}");
    assert!(std::fs::read(root.join(model_rel)).unwrap().len() > 100, "the half-written file was rewritten");
    assert_eq!(std::fs::read_to_string(root.join("drawings/Architectural/PRJ-001_AR_GROUND.dxf.bak")).unwrap(), "keep me");
}
