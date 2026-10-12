//! Sheets: the title block and its fields, saved sheet templates, and viewports.

use aad_engine::Session;
use aad_engine::doc::EntityKind;
use serde_json::{Value, json};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s.execute("project", &json!({"details": {"title": "Rao House", "owner": "S. Rao", "architect": "HC Works"}})).unwrap();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    s
}

fn layout_entities(s: &Session, layout: &str) -> Vec<(String, EntityKind)> {
    let d = s.doc().unwrap();
    d.layouts.iter().find(|l| l.name == layout).unwrap().entities.iter().map(|e| (e.common.layer.clone(), e.kind.clone())).collect()
}

fn fields(s: &mut Session) -> Value {
    s.execute("sheet.fields", &json!({})).unwrap()["fields"].clone()
}

#[test]
fn a_title_block_starts_from_the_project_and_its_fields_can_be_edited() {
    let mut s = session();
    let r = s.execute("sheet.titleblock", &json!({})).unwrap();
    assert_eq!(r["created"], true);
    assert_eq!(r["fields"]["PROJECT"], "Rao House");
    assert_eq!(r["fields"]["CLIENT"], "S. Rao");
    assert_eq!(r["fields"]["DRAWN"], "HC Works");
    assert_eq!(r["fields"]["SHEET"], "Layout1");
    assert!(s.doc().unwrap().block("AAD-TITLEBLOCK").is_some());
    let ents = layout_entities(&s, "Layout1");
    assert!(ents.iter().any(|(l, k)| l == "A-ANNO-BORD" && matches!(k, EntityKind::LwPolyline(_))), "the border");

    // The panel sits at the lower right of the printable area, at its real size in millimetres.
    let sheet = aad_engine::render::sheet(s.doc().unwrap(), "Layout1").unwrap();
    let ins = ents.iter().find_map(|(_, k)| if let EntityKind::Insert(i) = k { Some(i.clone()) } else { None }).unwrap();
    assert!((ins.insert.x - sheet.printable.max.x).abs() < 1e-9 && (ins.insert.y - sheet.printable.min.y).abs() < 1e-9);
    assert!((ins.scale.x - 1.0 / sheet.unit_mm).abs() < 1e-9);

    // Fields are set by tag, read back, and a second title block is not added.
    s.execute("sheet.fields", &json!({"fields": {"title": "Ground Floor Plan", "SHEET": "A-101", "rev": "B"}})).unwrap();
    let f = fields(&mut s);
    assert_eq!((f["TITLE"].as_str(), f["SHEET"].as_str(), f["REV"].as_str()), (Some("Ground Floor Plan"), Some("A-101"), Some("B")));
    let again = s.execute("sheet.titleblock", &json!({"fields": {"DATE": "2026-10-11"}})).unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(again["fields"]["DATE"], "2026-10-11");
    assert_eq!(again["fields"]["SHEET"], "A-101", "edited fields are kept");
    assert_eq!(layout_entities(&s, "Layout1").iter().filter(|(_, k)| matches!(k, EntityKind::Insert(_))).count(), 1);

    // Refusals.
    assert!(s.execute("sheet.fields", &json!({"fields": {"TITLE": {"a": 1}}})).is_err());
    assert!(s.execute("sheet.fields", &json!({"fields": {"TITLE": "x".repeat(600)}})).is_err());
    let blank = s.execute("layout.new", &json!({"viewport": false})).unwrap()["name"].as_str().unwrap().to_string();
    assert!(s.execute("sheet.fields", &json!({"layout": blank, "fields": {"TITLE": "x"}})).is_err(), "no title block on the new sheet");
    s.execute("layout.set", &json!({"name": "Model"})).unwrap();
    assert!(s.execute("sheet.titleblock", &json!({})).is_err(), "not in the Model tab");
    assert!(s.execute("sheet.titleblock", &json!({"layout": "Nope"})).is_err());
}

#[test]
fn viewports_are_listed_added_scaled_and_removed() {
    let mut s = session();
    s.execute("line", &json!({"points": [[0, 0], [20000, 0]]})).ok();
    s.execute("sheet.titleblock", &json!({})).unwrap();
    let r = s.execute("viewport.add", &json!({"scale": "1:100"})).unwrap();
    let h = r["handle"].as_str().unwrap().to_string();
    let l = s.execute("viewport.list", &json!({})).unwrap();
    let v = &l["viewports"][0];
    assert_eq!((v["handle"].as_str(), v["scaleText"].as_str()), (Some(h.as_str()), Some("1:100")));
    let sheet = aad_engine::render::sheet(s.doc().unwrap(), "Layout1").unwrap();
    let rect: Vec<f64> = v["rect"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert!((rect[1] - (sheet.printable.min.y + 56.0 / sheet.unit_mm)).abs() < 1e-6, "above the title block: {rect:?}");
    assert_eq!(s.execute("sheet.fields", &json!({})).unwrap()["fields"]["SCALE"], "1:100", "the title block shows the new viewport's scale");
    s.execute("viewport.set", &json!({"handle": h, "scale": "1:50", "locked": true})).unwrap();
    assert_eq!(s.execute("viewport.list", &json!({})).unwrap()["viewports"][0]["scaleText"], "1:50");
    assert_eq!(s.execute("viewport.list", &json!({})).unwrap()["viewports"][0]["locked"], true);

    assert!(s.execute("viewport.remove", &json!({"handle": "FFFF"})).is_err(), "not a viewport");
    assert!(s.execute("viewport.remove", &json!({})).is_err());
    s.execute("viewport.remove", &json!({"handle": h})).unwrap();
    assert_eq!(s.execute("viewport.list", &json!({})).unwrap()["viewports"].as_array().unwrap().len(), 0);
}

#[test]
fn a_saved_sheet_is_applied_as_a_title_card_and_replaced_without_touching_the_users_work() {
    let mut s = session();
    s.execute("pagesetup", &json!({"paper": "A3", "landscape": true})).unwrap();
    s.execute("sheet.titleblock", &json!({"fields": {"REV": "A"}})).unwrap();
    s.execute("viewport.add", &json!({"scale": "1:100"})).unwrap();
    let saved = s.execute("sheet.template.save", &json!({"name": "A3 office"})).unwrap();
    assert!(saved["entities"].as_u64().unwrap() >= 2 && saved["blocks"].as_u64().unwrap() >= 1);
    let list = s.execute("sheet.template.list", &json!({})).unwrap();
    assert_eq!((list["templates"][0]["name"].as_str(), list["templates"][0]["scope"].as_str()), (Some("A3 office"), Some("drawing")));

    // A new sheet gets the page setup, the border and the title block, filled from the project.
    let name = s.execute("layout.new", &json!({"viewport": false})).unwrap()["name"].as_str().unwrap().to_string();
    let before = layout_entities(&s, &name).len();
    let r = s.execute("sheet.template.apply", &json!({"name": "A3 office", "layout": name, "fields": {"TITLE": "First Floor Plan"}})).unwrap();
    assert!(r["placed"].as_u64().unwrap() >= 2);
    assert_eq!(r["replaced"], 0);
    let page = s.doc().unwrap().layouts.iter().find(|l| l.name == name).unwrap().page.clone();
    assert!(page.landscape && page.paper.contains("A3"), "{}", page.paper);
    let f = s.execute("sheet.fields", &json!({"layout": name})).unwrap()["fields"].clone();
    assert_eq!((f["PROJECT"].as_str(), f["TITLE"].as_str(), f["REV"].as_str()), (Some("Rao House"), Some("First Floor Plan"), Some("A")));
    assert!(layout_entities(&s, &name).len() > before);
    assert!(layout_entities(&s, &name).iter().all(|(_, k)| !matches!(k, EntityKind::Viewport(v) if v.id != 1)), "templates carry no viewports");

    // The user draws a note; applying the template again replaces only what the template placed.
    s.execute("layout.set", &json!({"name": name})).unwrap();
    s.execute("line", &json!({"points": [[10, 10], [60, 10]]})).unwrap();
    let count = layout_entities(&s, &name).len();
    let r = s.execute("sheet.template.apply", &json!({"name": "A3 office", "layout": name})).unwrap();
    assert!(r["replaced"].as_u64().unwrap() >= 2);
    assert_eq!(layout_entities(&s, &name).len(), count, "no duplicates, and the user's line stayed");
    assert!(layout_entities(&s, &name).iter().any(|(_, k)| matches!(k, EntityKind::Line(_))));

    // Unknown names and damaged templates are refused and change nothing.
    assert!(s.execute("sheet.template.apply", &json!({"name": "Nope"})).is_err());
    let mut all = s.doc().unwrap().project.get("sheetTemplates").cloned().map(|t| serde_json::from_str::<Value>(&t).unwrap()).unwrap();
    all["Broken"] = json!({"format": "aad-sheet-template", "page": 3, "entities": []});
    all["Bad object"] = json!({"format": "aad-sheet-template", "page": all["A3 office"]["page"].clone(), "entities": [{"nonsense": true}]});
    s.doc_mut().unwrap().project.insert("sheetTemplates".into(), all.to_string());
    for n in ["Broken", "Bad object"] {
        let c = layout_entities(&s, &name).len();
        assert!(s.execute("sheet.template.apply", &json!({"name": n, "layout": name})).is_err(), "{n}");
        assert_eq!(layout_entities(&s, &name).len(), c, "{n} changed nothing");
    }
    assert!(s.execute("sheet.template.save", &json!({"name": ""})).is_err());
    assert!(s.execute("sheet.template.save", &json!({"name": "x", "scope": "cloud"})).is_err());
    s.execute("sheet.template.delete", &json!({"name": "Broken"})).unwrap();
    assert!(s.execute("sheet.template.delete", &json!({"name": "Broken"})).is_err());
}

#[test]
fn templates_can_live_in_a_shared_folder_for_the_whole_office() {
    let mut s = session();
    let dir = std::env::temp_dir().join(format!("aad-sheet-tpl-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let folder = dir.to_str().unwrap();
    s.execute("sheet.titleblock", &json!({})).unwrap();
    s.execute("sheet.template.save", &json!({"name": "Office A4", "scope": "user", "folder": folder})).unwrap();
    assert!(dir.join("Office A4.json").is_file());
    let l = s.execute("sheet.template.list", &json!({"folder": folder})).unwrap();
    assert!(l["templates"].as_array().unwrap().iter().any(|t| t["name"] == "Office A4" && t["scope"] == "user"));
    // Another drawing finds it and applies it.
    let mut t = session();
    t.execute("layout.new", &json!({})).unwrap();
    t.execute("sheet.template.apply", &json!({"name": "Office A4", "folder": folder})).unwrap();
    assert_eq!(t.execute("sheet.fields", &json!({})).unwrap()["fields"]["PROJECT"], "Rao House");
    t.execute("sheet.template.delete", &json!({"name": "Office A4", "scope": "user", "folder": folder})).unwrap();
    assert!(!dir.join("Office A4.json").exists());
    // A junk file in the folder is ignored, not a crash.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("junk.json"), "{not json").unwrap();
    assert!(s.execute("sheet.template.list", &json!({"folder": folder})).unwrap()["templates"].as_array().unwrap().is_empty());
    std::fs::remove_dir_all(&dir).ok();
}
