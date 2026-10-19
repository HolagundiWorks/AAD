//! `takeoff.record`: the take-off as ShilpiDB records (contract: `TAKEOFF-SHILPIDB.md` in the O-CoreOp
//! repository, version 1). The tests follow the hand-off's list: the records match the bridge rows, recording
//! again replaces, a removed item is deleted, ids carry the kind, everything is in millimetres, a rectangular slab
//! has its spans, and refs survive a rebuilt take-off.

use aad_engine::Session;
use aad_engine::doc::{EntityKind, Handle};
use serde_json::{Value, json};

fn session(units: &str) -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": units, "mode": "metadata"})).unwrap();
    s
}

fn last_handle(s: &Session) -> Handle {
    s.doc().unwrap().model.iter().last().unwrap().handle
}

fn layer(s: &mut Session, name: &str) {
    s.execute("layer.current", &json!({"name": name})).unwrap();
}

/// Handles of what the scene measured, so a test can change or remove one thing.
struct Scene {
    wall_a: Handle,
    wall_b: Handle,
}

/// A small house in millimetres: two measured walls, a door and a window, a column, a lintel, a rectangular slab
/// and a room. Take-off tables are not drawn.
fn scene(s: &mut Session) -> Scene {
    s.execute("floors", &json!({"floors": [{"name": "Ground", "floorToFloor": 3.15, "ceiling": 3.0, "lintel": 2.1, "slab": 150, "beam": 450}]}))
        .unwrap();
    s.execute("openings.std", &json!({})).unwrap();
    s.execute("wall.draw", &json!({"points": [[0, 0], [9000, 0], [9000, 6000], [0, 6000], [0, 0]], "thickness": 230})).unwrap();
    s.execute("wall.door", &json!({"at": [2000, 0], "name": "D2", "width": 900})).unwrap();
    s.execute("wall.window", &json!({"at": [5000, 0], "name": "W3", "width": 1200})).unwrap();
    s.execute("takeoff.start", &json!({})).unwrap();
    layer(s, "MEASURE-FULLBRICK");
    s.execute("line", &json!({"points": [[0, 0], [9000, 0]]})).unwrap();
    let wall_a = last_handle(s);
    s.execute("line", &json!({"points": [[0, 6000], [9000, 6000]]})).unwrap();
    let wall_b = last_handle(s);
    layer(s, "MEASURE-COLUMN");
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [230, 450]})).unwrap();
    layer(s, "MEASURE-LINTEL");
    s.execute("line", &json!({"points": [[1550, 0], [2450, 0]]})).unwrap();
    layer(s, "MEASURE-SLAB");
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [9000, 6000]})).unwrap();
    layer(s, "0");
    s.execute("room", &json!({"p1": [100, 100], "p2": [4100, 3100], "name": "br", "units": "mm", "textHeight": 150})).unwrap();
    measure(s);
    Scene { wall_a, wall_b }
}

/// Run every take-off tool again from the geometry on the measure layers.
fn measure(s: &mut Session) {
    s.execute("takeoff.brick", &json!({"table": false})).unwrap();
    s.execute("takeoff.columns", &json!({"table": false})).unwrap();
    s.execute("takeoff.beams", &json!({"table": false})).unwrap();
    s.execute("takeoff.slab", &json!({"table": false})).unwrap();
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aad-takeoff-record-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("house.takeoff.vdb")
}

fn record(s: &mut Session, path: &std::path::Path) -> Value {
    s.execute("takeoff.record", &json!({"path": path.to_string_lossy(), "annotate": false})).unwrap()
}

/// The records of a database: id → (payload as JSON, box).
fn read(path: &std::path::Path) -> std::collections::BTreeMap<u64, (Value, [f64; 4])> {
    let store = aad_vdb::load_path(path).unwrap();
    store
        .records()
        .iter()
        .map(|r| {
            let b = r.bbox;
            (r.id, (serde_json::from_slice(&r.payload).unwrap_or(Value::Null), [b.min.x, b.min.y, b.max.x, b.max.y]))
        })
        .collect()
}

fn kind(id: u64) -> u64 {
    id >> 56
}

fn slot(id: u64) -> u64 {
    (id >> 32) & 0x00FF_FFFF
}

/// The id of an item whose entity handle is `n` (small handles keep their value as the number) in `slot`.
fn item_id(kind: u64, slot: u64, n: u64) -> u64 {
    (kind << 56) | (slot << 32) | n
}

/// The one header in a database.
fn header_id(db: &std::collections::BTreeMap<u64, (Value, [f64; 4])>) -> u64 {
    let mut ids = db.keys().filter(|id| kind(**id) == 1);
    let id = *ids.next().expect("a header record");
    assert!(ids.next().is_none(), "one header per drawing");
    id
}

fn cleanup(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        std::fs::remove_dir_all(dir).ok();
    }
}

#[test]
fn the_records_match_the_bridge_rows_and_the_header_is_complete() {
    let mut s = session("mm");
    scene(&mut s);
    let path = temp("rows");
    let r = record(&mut s, &path);
    let db = read(&path);
    let hid = header_id(&db);
    assert_eq!(r["headerId"], json!(hid));
    assert_eq!(hid, item_id(1, slot(hid), 0), "the header is number 0 of its slot");
    assert!((1..=0x00FF_FFFF).contains(&slot(hid)));

    let header = &db.get(&hid).expect("a header record").0;
    assert_eq!(header["state"], "complete", "a finished recording is complete");
    assert!(header.get("annotated_pdf").is_none(), "no PDF was asked for");
    assert_eq!((header["format"].as_str(), header["version"].as_u64(), header["units"].as_str()), (Some("hcw-takeoff-shilpi"), Some(1), Some("mm")));
    assert_eq!(header["source"]["app"], "O-Draft");
    assert!(header["source"]["drawing_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(header["levels"][0]["name"], "Ground");
    assert!(header["opening_schedule"].as_array().is_some_and(|a| !a.is_empty()));
    assert_eq!(db.keys().filter(|id| kind(**id) == 1).count(), 1, "exactly one header");

    // The same rows `bridge.export` writes.
    let exported: Value = serde_json::from_str(s.execute("bridge.export", &json!({"write": false})).unwrap()["json"].as_str().unwrap()).unwrap();
    for (section, k) in [("walls", 3), ("openings", 4), ("columns", 5), ("lintels", 6), ("rooms", 7), ("slabs", 8)] {
        let rows = exported[section].as_array().unwrap();
        let recorded: Vec<&Value> = db.iter().filter(|(id, _)| kind(**id) == k).map(|(_, (p, _))| p).collect();
        assert_eq!(recorded.len(), rows.len(), "{section}: one record per row");
        assert!(!rows.is_empty(), "{section}: the scene has some");
        for row in rows {
            assert!(recorded.contains(&row), "{section}: the record equals the bridge row {row}");
        }
    }
    assert_eq!(r["counts"]["walls"], 2);
    assert_eq!(r["counts"]["openings"], 2);
    assert_eq!(r["counts"]["rooms"], 1);
    cleanup(&path);
}

#[test]
fn ids_carry_the_kind_the_slot_and_the_number() {
    let mut s = session("mm");
    scene(&mut s);
    let path = temp("ids");
    record(&mut s, &path);
    let db = read(&path);
    let sl = slot(header_id(&db));
    for (id, (payload, _)) in db {
        let k = kind(id);
        assert_eq!(slot(id), sl, "one drawing, one slot");
        assert!([1, 3, 4, 5, 6, 7, 8].contains(&k), "kind {k}");
        if k != 1 {
            // The ref names the entity handle the number came from: W<hex>, O<hex>, C<hex>, L<hex>, R<hex>, S<hex>.
            let r = payload["ref"].as_str().unwrap();
            let handle = u64::from_str_radix(&r[1..], 16).unwrap();
            assert_eq!(id & 0xFFFF_FFFF, handle, "{r}");
        }
    }
    cleanup(&path);
}

#[test]
fn recording_again_replaces_and_a_changed_wall_changes_one_record() {
    let mut s = session("mm");
    let scene = scene(&mut s);
    let path = temp("again");
    let first = record(&mut s, &path);
    let before = read(&path);
    let second = record(&mut s, &path);
    let same = read(&path);
    assert_eq!(before.keys().collect::<Vec<_>>(), same.keys().collect::<Vec<_>>(), "the same ids");
    assert_eq!(before.len(), same.len(), "no duplicates");
    assert_eq!((second["added"].as_u64(), second["changed"].as_u64(), second["deleted"].as_u64()), (Some(0), Some(0), Some(0)), "{second}");
    assert_eq!(second["unchanged"].as_u64(), first["written"].as_u64().map(|n| n - 1), "every row but the header is unchanged");

    // Make one wall 1 m longer, measure again, record again: that wall's record changes, nothing else.
    s.doc_mut()
        .unwrap()
        .modify_entity(scene.wall_a, |e| {
            if let EntityKind::Line(l) = &mut e.kind {
                l.b.x = 10_000.0;
            }
        })
        .unwrap();
    measure(&mut s);
    let third = record(&mut s, &path);
    assert_eq!((third["added"].as_u64(), third["changed"].as_u64(), third["deleted"].as_u64()), (Some(0), Some(1), Some(0)), "{third}");
    let after = read(&path);
    let id = item_id(3, slot(header_id(&after)), scene.wall_a.0);
    assert_eq!(after[&id].0["length_mm"], json!(10_000.0));
    assert_eq!(before[&id].0["length_mm"], json!(9000.0));
    cleanup(&path);
}

#[test]
fn a_wall_that_is_no_longer_measured_is_deleted_from_the_database() {
    let mut s = session("mm");
    let scene = scene(&mut s);
    let path = temp("gone");
    record(&mut s, &path);
    let before = read(&path);
    let id = item_id(3, slot(header_id(&before)), scene.wall_b.0);
    assert!(before.contains_key(&id));
    s.execute("erase", &json!({"handles": [scene.wall_b.hex()]})).unwrap();
    measure(&mut s);
    let r = record(&mut s, &path);
    assert_eq!(r["deleted"], 1, "{r}");
    let db = read(&path);
    assert!(!db.contains_key(&id), "its record is gone");
    assert_eq!(db.keys().filter(|id| kind(**id) == 3).count(), 1);
    cleanup(&path);
}

#[test]
fn refs_survive_a_rebuilt_take_off() {
    let mut s = session("mm");
    scene(&mut s);
    let path = temp("stable");
    record(&mut s, &path);
    let before: Vec<u64> = read(&path).keys().copied().collect();
    // Clear the saved results and measure the same geometry again.
    s.execute("takeoff.clear", &json!({})).ok();
    measure(&mut s);
    record(&mut s, &path);
    let after: Vec<u64> = read(&path).keys().copied().collect();
    assert_eq!(before, after, "the same ids for the same things");
    cleanup(&path);
}

#[test]
fn everything_is_in_millimetres_whatever_the_drawing_units() {
    // A 10 ft wall drawn in inches: 120 in = 3048 mm.
    let mut s = session("in");
    s.execute("takeoff.start", &json!({})).unwrap();
    layer(&mut s, "MEASURE-FULLBRICK");
    s.execute("line", &json!({"points": [[0, 0], [120, 0]]})).unwrap();
    s.execute("takeoff.brick", &json!({"table": false})).unwrap();
    let path = temp("imperial");
    record(&mut s, &path);
    let db = read(&path);
    assert_eq!(db[&header_id(&db)].0["units"], "mm");
    let (_, (wall, bbox)) = db.iter().find(|(id, _)| kind(**id) == 3).unwrap();
    let len = wall["length_mm"].as_f64().unwrap();
    assert!((len - 3048.0).abs() < 0.5, "length {len}");
    assert!((bbox[2] - 3048.0).abs() < 0.5 && bbox[0].abs() < 0.5, "box {bbox:?}");
    cleanup(&path);
}

#[test]
fn a_rectangular_slab_carries_its_spans_and_an_l_shaped_one_does_not() {
    let mut s = session("mm");
    scene(&mut s);
    // An L-shaped slab on the same layer.
    layer(&mut s, "MEASURE-SLAB");
    s.execute(
        "pline",
        &json!({"points": [[20_000, 0], [26_000, 0], [26_000, 2000], [22_000, 2000], [22_000, 5000], [20_000, 5000]], "closed": true}),
    )
    .unwrap();
    layer(&mut s, "0");
    measure(&mut s);
    let path = temp("slab");
    record(&mut s, &path);
    let slabs: Vec<Value> = read(&path).into_iter().filter(|(id, _)| kind(*id) == 8).map(|(_, (p, _))| p).collect();
    assert_eq!(slabs.len(), 2);
    let rect = slabs.iter().find(|s| s.get("rectangular").is_some()).expect("the rectangle has spans");
    assert_eq!((rect["length_mm"].as_f64(), rect["breadth_mm"].as_f64(), rect["rectangular"].as_bool()), (Some(9000.0), Some(6000.0), Some(true)));
    let l = slabs.iter().find(|s| s.get("rectangular").is_none()).expect("the L has none");
    assert!(l.get("length_mm").is_none() && l.get("breadth_mm").is_none());
    assert!(l["area_m2"].as_f64().unwrap() > 0.0, "the area is still there");
    cleanup(&path);
}

#[test]
fn the_default_file_sits_beside_the_drawing_and_an_entity_store_is_never_overwritten() {
    let mut s = session("mm");
    scene(&mut s);
    let dir = std::env::temp_dir().join(format!("aad-takeoff-record-{}-beside", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // No path and no saved drawing: an error that says what to do.
    assert!(s.execute("takeoff.record", &json!({"file": true})).is_err());
    s.state_mut().unwrap().path = Some(dir.join("house.dxf").to_string_lossy().to_string());
    let r = s.execute("takeoff.record", &json!({"file": true, "annotate": false})).unwrap();
    assert!(r["path"].as_str().unwrap().ends_with("house.takeoff.vdb"), "{r}");
    assert!(dir.join("house.takeoff.vdb").exists());
    // A .vdb that holds entities, not a take-off, is refused.
    let other = dir.join("entities.vdb");
    s.execute("vdb.export", &json!({"path": other.to_string_lossy()})).unwrap();
    let before = std::fs::read(&other).unwrap();
    let err = s.execute("takeoff.record", &json!({"path": other.to_string_lossy()})).unwrap_err().to_string();
    assert!(err.contains("not a take-off database"), "{err}");
    assert_eq!(std::fs::read(&other).unwrap(), before, "the file was not touched");
    // A server that is not there is an error, not a hang or a panic.
    assert!(s.execute("takeoff.record", &json!({"server": "127.0.0.1:1"})).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_empty_drawing_records_just_the_header() {
    let mut s = session("mm");
    let path = temp("empty");
    let r = record(&mut s, &path);
    assert_eq!(r["written"], 1);
    let db = read(&path);
    assert_eq!(db.len(), 1);
    assert_eq!(db[&header_id(&db)].0["state"], "complete");
    cleanup(&path);
}

#[test]
fn the_slot_is_kept_in_the_drawing_and_two_drawings_share_a_database_without_touching_each_other() {
    let path = temp("two");
    let mut a = session("mm");
    scene(&mut a);
    let first = record(&mut a, &path);
    let mut b = session("mm");
    b.execute("takeoff.start", &json!({})).unwrap();
    layer(&mut b, "MEASURE-FULLBRICK");
    b.execute("line", &json!({"points": [[0, 0], [4000, 0]]})).unwrap();
    b.execute("takeoff.brick", &json!({"table": false})).unwrap();
    let second = record(&mut b, &path);
    assert_ne!(first["slot"], second["slot"], "two drawings, two slots");
    let db = read(&path);
    assert_eq!(db.keys().filter(|id| kind(**id) == 1).count(), 2, "two headers");
    let (sa, sb) = (first["slot"].as_u64().unwrap(), second["slot"].as_u64().unwrap());
    assert_eq!(db.keys().filter(|id| slot(**id) == sb && kind(**id) == 3).count(), 1);
    let rows_of_a = db.keys().filter(|id| slot(**id) == sa).count();

    // Recording A again does not touch B, and A keeps its slot.
    let again = record(&mut a, &path);
    assert_eq!(again["slot"], first["slot"], "the slot does not change");
    let db2 = read(&path);
    assert_eq!(db2.keys().filter(|id| slot(**id) == sa).count(), rows_of_a);
    assert_eq!(db2.keys().filter(|id| slot(**id) == sb).count(), db.keys().filter(|id| slot(**id) == sb).count());
    cleanup(&path);
}

/// Against a real `shilpid`: `AAD_SHILPID_ADDR=127.0.0.1:7430 cargo test -p aad-engine --test takeoff_record -- --ignored`
/// (start it with `shilpid --bind 127.0.0.1:7430 --data x.vdb`). Not part of CI: it needs the server program.
#[test]
#[ignore = "needs a running shilpid; see the doc comment"]
fn a_running_shilpid_ends_up_with_the_take_off_and_loses_removed_rows() {
    let Ok(addr) = std::env::var("AAD_SHILPID_ADDR") else { return };
    let mut s = session("mm");
    let scene = scene(&mut s);
    let first = s.execute("takeoff.record", &json!({"server": addr, "annotate": false})).unwrap();
    let mut client = aad_vdb::VdbClient::connect(addr.as_str()).unwrap();
    let ids = client.list().unwrap();
    assert_eq!(ids.len() as u64, first["written"].as_u64().unwrap());
    let hid = first["headerId"].as_u64().unwrap();
    assert!(ids.contains(&hid));
    let again = s.execute("takeoff.record", &json!({"server": addr, "annotate": false})).unwrap();
    assert_eq!(again["added"], 0, "recording again replaces by id: {again}");
    assert_eq!(client.list().unwrap().len(), ids.len(), "no duplicates");
    s.execute("erase", &json!({"handles": [scene.wall_b.hex()]})).unwrap();
    measure(&mut s);
    let third = s.execute("takeoff.record", &json!({"server": addr, "annotate": false})).unwrap();
    assert_eq!(third["deleted"], 1, "{third}");
    assert_eq!(client.list().unwrap().len(), ids.len() - 1);
    let wall_a = client.get(item_id(3, slot(hid), scene.wall_a.0)).unwrap().expect("the remaining wall");
    let payload: Value = serde_json::from_slice(&wall_a.payload).unwrap();
    assert_eq!(payload["length_mm"], 9000.0);
}

/// A plotter that keeps what it was asked to plot and returns a PDF-looking buffer of known size.
static PLOTTED: std::sync::Mutex<Vec<aad_engine::doc::Drawing>> = std::sync::Mutex::new(Vec::new());

fn fake_plot(d: &aad_engine::doc::Drawing, _: &aad_engine::doc::Space, _: &Value) -> Result<Vec<u8>, String> {
    PLOTTED.lock().unwrap().push(d.clone());
    let mut pdf = b"%PDF-1.4\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\n".to_vec();
    pdf.resize(1_200_000, b'x');
    Ok(pdf)
}

/// The annotated drawing: every measured item is drawn again on `TAKEOFF-ANNOT` with a label, the drawing itself is
/// untouched, and the PDF is stored in chunks that add up to the file the header describes.
#[test]
fn the_annotated_drawing_marks_what_was_measured_and_is_stored_in_chunks() {
    aad_engine::cmd::file::set_io(aad_engine::cmd::file::IoHooks {
        read: |_, _| Err("no".into()),
        write: |_, _| Ok(Vec::new()),
        plot: Some(fake_plot),
    });
    let mut s = session("mm");
    scene(&mut s);
    let before = s.doc().unwrap().model.len();
    let path = temp("annotate");
    let r = s.execute("takeoff.record", &json!({"path": path.to_string_lossy()})).unwrap();
    assert_eq!(r["state"], "complete");
    assert_eq!(s.doc().unwrap().model.len(), before, "the drawing itself is untouched");

    // What the plotter was given: the measured items again, on the annotation layer, plus labels, legend and title.
    let plotted = PLOTTED.lock().unwrap().pop().expect("the plotter was called");
    let drawn: Vec<_> = plotted.model.iter().filter(|e| e.common.layer == "TAKEOFF-ANNOT").collect();
    let texts = drawn.iter().filter(|e| matches!(e.kind, EntityKind::Text(_))).count();
    let shapes = drawn.len() - texts;
    assert!(shapes >= 7, "walls, openings, column, lintel, room and slab are drawn again: {shapes}");
    assert!(texts > shapes, "each has a label, and the legend and title add more: {texts}");
    assert!(drawn.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value.starts_with("Walls: 2"))), "the legend counts the walls");

    // The stored PDF: chunk records of kind 9 that add up to what the header says.
    let db = read(&path);
    let header = &db[&header_id(&db)].0;
    let p = &header["annotated_pdf"];
    assert_eq!(p["bytes"], 1_200_000);
    assert_eq!(p["pages"], 1);
    let mut chunks: Vec<(u64, usize)> = Vec::new();
    for (id, _) in db.iter().filter(|(id, _)| kind(**id) == 9) {
        let rec = aad_vdb::load_path(&path).unwrap();
        let r = rec.records().iter().find(|r| r.id == *id).unwrap();
        chunks.push((*id & 0xFFFF_FFFF, r.payload.len()));
    }
    chunks.sort();
    assert_eq!(p["chunks"].as_u64(), Some(chunks.len() as u64));
    assert_eq!(chunks.len(), 3, "1.2 MB in chunks of at most 512 KiB");
    assert!(chunks.iter().enumerate().all(|(i, c)| c.0 == i as u64 && c.1 <= 512 * 1024));
    assert_eq!(chunks.iter().map(|c| c.1).sum::<usize>(), 1_200_000);

    // Recording again without the PDF drops the old chunks and the header's entry.
    s.execute("takeoff.record", &json!({"path": path.to_string_lossy(), "annotate": false})).unwrap();
    let db = read(&path);
    assert_eq!(db.keys().filter(|id| kind(**id) == 9).count(), 0);
    assert!(db[&header_id(&db)].0.get("annotated_pdf").is_none());
    cleanup(&path);
}

/// What O-CoreOp asked for after reading the first recording: rooms carry their level and sides, openings name their
/// wall, walls carry a thickness, and a ventilator is a window.
#[test]
fn rows_carry_what_o_coreop_needs_to_use_them_without_guessing() {
    let mut s = session("mm");
    let scene = scene(&mut s);
    let path = temp("needs");
    let r = record(&mut s, &path);
    let db = read(&path);
    let rows = |k: u64| -> Vec<Value> { db.iter().filter(|(id, _)| kind(**id) == k).map(|(_, (p, _))| p.clone()).collect() };

    // Walls: the thickness the layer implies.
    for w in rows(3) {
        assert_eq!(w["thickness_mm"], json!(230.0), "{w}");
    }
    assert!(
        r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().is_some_and(|w| w.contains("nominal"))),
        "the thickness is said to be nominal"
    );

    // Rooms: level and both sides, rectangular.
    let room = &rows(7)[0];
    assert_eq!(room["level"], "Ground");
    assert_eq!(
        (room["length_mm"].as_f64(), room["breadth_mm"].as_f64(), room["rectangular"].as_bool()),
        (Some(4000.0), Some(3000.0), Some(true)),
        "{room}"
    );

    // Openings: both sit on the front wall (y = 0).
    let front = format!("W{}", scene.wall_a.hex());
    for o in rows(4) {
        assert_eq!(o["wall_ref"], json!(front), "{o}");
    }
    assert!(!r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().is_some_and(|w| w.contains("not on a measured wall"))));

    // An opening far from every measured wall gets no wall_ref, and the export says so.
    s.execute("wall.window", &json!({"at": [4500, 3000], "name": "W3", "width": 1200, "angle": 0})).unwrap();
    let again = record(&mut s, &path);
    let db = read(&path);
    let stray: Vec<Value> =
        db.iter().filter(|(id, _)| kind(**id) == 4).map(|(_, (p, _))| p.clone()).filter(|o| o.get("wall_ref").is_none()).collect();
    assert_eq!(stray.len(), 1, "{stray:?}");
    assert!(again["warnings"].as_array().unwrap().iter().any(|w| w.as_str().is_some_and(|w| w.contains("1 opening(s) are not on a measured wall"))));
    cleanup(&path);
}

#[test]
fn the_header_names_the_company_file_project_so_o_coreop_can_match_the_take_off() {
    let mut s = session("mm");
    scene(&mut s);
    let path = temp("project-ref");
    let db = read(&path_after(&mut s, &path));
    assert!(db[&header_id(&db)].0["source"].get("project_ref").is_none(), "an unlinked drawing names no project");

    let context = json!({
        "v": 1, "prepared_on": "2026-10-20", "company": { "name": "Studio One" },
        "project": { "project": { "name": "Mehta residence" }, "client": { "name": "Mehta family" }, "project_ref": { "v": 1, "company_uuid": "u-1", "project_id": 7 } }
    });
    s.execute("companyfile", &json!({ "context": context })).unwrap();
    let db = read(&path_after(&mut s, &path));
    assert_eq!(db[&header_id(&db)].0["source"]["project_ref"], json!({ "v": 1, "company_uuid": "u-1", "project_id": 7 }));
    cleanup(&path);
}

/// Records the take-off and returns where it went.
fn path_after(s: &mut Session, path: &std::path::Path) -> std::path::PathBuf {
    record(s, path);
    path.to_path_buf()
}
