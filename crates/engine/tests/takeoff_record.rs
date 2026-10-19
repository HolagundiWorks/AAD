//! `takeoff.record`: the take-off as ShilpiDB records (contract: `TAKEOFF-SHILPIDB.md` in the O-CoreOp
//! repository, version 1). The tests follow the hand-off's list: the records match the bridge rows, recording
//! again replaces, a removed item is deleted, ids carry the kind, everything is in millimetres, a rectangular slab
//! has its spans, and refs survive a rebuilt take-off.

use aad_engine::Session;
use aad_engine::doc::{EntityKind, Handle};
use serde_json::{Value, json};

const HEADER_ID: u64 = 72_057_594_037_927_936;

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
    s.execute("takeoff.record", &json!({"path": path.to_string_lossy()})).unwrap()
}

/// The records of a database: id → (payload as JSON, box).
fn read(path: &std::path::Path) -> std::collections::BTreeMap<u64, (Value, [f64; 4])> {
    let store = aad_vdb::load_path(path).unwrap();
    store
        .records()
        .iter()
        .map(|r| {
            let b = r.bbox;
            (r.id, (serde_json::from_slice(&r.payload).unwrap(), [b.min.x, b.min.y, b.max.x, b.max.y]))
        })
        .collect()
}

fn kind(id: u64) -> u64 {
    id >> 56
}

fn cleanup(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        std::fs::remove_dir_all(dir).ok();
    }
}

#[test]
fn the_records_match_the_bridge_rows_and_the_header_is_at_the_fixed_id() {
    let mut s = session("mm");
    scene(&mut s);
    let path = temp("rows");
    let r = record(&mut s, &path);
    assert_eq!(r["headerId"], json!(HEADER_ID));
    let db = read(&path);

    let header = &db.get(&HEADER_ID).expect("a header record").0;
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
fn ids_carry_the_kind_in_the_top_byte_and_the_number_in_the_low_bits() {
    let mut s = session("mm");
    scene(&mut s);
    let path = temp("ids");
    record(&mut s, &path);
    for (id, (payload, _)) in read(&path) {
        let k = kind(id);
        assert!([1, 3, 4, 5, 6, 7, 8].contains(&k), "kind {k}");
        if k != 1 {
            // The ref names the entity handle the number came from: W<hex>, O<hex>, C<hex>, L<hex>, R<hex>, S<hex>.
            let r = payload["ref"].as_str().unwrap();
            let handle = u64::from_str_radix(&r[1..], 16).unwrap();
            assert_eq!(id & 0x00FF_FFFF_FFFF_FFFF, handle, "{r}");
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
    assert_eq!(second["unchanged"], first["written"]);

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
    let id = (3u64 << 56) | scene.wall_a.0;
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
    let id = (3u64 << 56) | scene.wall_b.0;
    assert!(read(&path).contains_key(&id));
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
    assert_eq!(db[&HEADER_ID].0["units"], "mm");
    let (id, (wall, bbox)) = db.iter().find(|(id, _)| kind(**id) == 3).unwrap();
    assert!(*id > HEADER_ID);
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
    assert!(s.execute("takeoff.record", &json!({})).is_err());
    s.state_mut().unwrap().path = Some(dir.join("house.dxf").to_string_lossy().to_string());
    let r = s.execute("takeoff.record", &json!({})).unwrap();
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
    assert!(db.contains_key(&HEADER_ID));
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
    let first = s.execute("takeoff.record", &json!({"server": addr})).unwrap();
    let mut client = aad_vdb::VdbClient::connect(addr.as_str()).unwrap();
    let ids = client.list().unwrap();
    assert_eq!(ids.len() as u64, first["written"].as_u64().unwrap());
    assert!(ids.contains(&HEADER_ID));
    let again = s.execute("takeoff.record", &json!({"server": addr})).unwrap();
    assert_eq!(again["added"], 0, "recording again replaces by id: {again}");
    assert_eq!(client.list().unwrap().len(), ids.len(), "no duplicates");
    s.execute("erase", &json!({"handles": [scene.wall_b.hex()]})).unwrap();
    measure(&mut s);
    let third = s.execute("takeoff.record", &json!({"server": addr})).unwrap();
    assert_eq!(third["deleted"], 1, "{third}");
    assert_eq!(client.list().unwrap().len(), ids.len() - 1);
    let wall_a = client.get((3u64 << 56) | scene.wall_a.0).unwrap().expect("the remaining wall");
    let payload: Value = serde_json::from_slice(&wall_a.payload).unwrap();
    assert_eq!(payload["length_mm"], 9000.0);
}
