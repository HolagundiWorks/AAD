//! The annotated take-off drawing with the real PDF writer: the plotter the application installs, not a stand-in.
//! `takeoff.record` stores the PDF in chunks; they must reassemble into the PDF the header describes, and a
//! recording against a real `shilpid` must hold the same.

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

fn layer(s: &mut Session, name: &str) {
    s.execute("layer.current", &json!({"name": name})).unwrap();
}

/// A drawing with measured walls, a slab, a column and a room.
fn house() -> Session {
    install_io();
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s.execute("floors", &json!({"floors": [{"name": "Ground", "floorToFloor": 3.15, "ceiling": 3.0, "lintel": 2.1, "slab": 150, "beam": 450}]}))
        .unwrap();
    s.execute("openings.std", &json!({})).unwrap();
    s.execute("wall.draw", &json!({"points": [[0, 0], [9000, 0], [9000, 6000], [0, 6000], [0, 0]], "thickness": 230})).unwrap();
    s.execute("wall.door", &json!({"at": [2000, 0], "name": "D2", "width": 900})).unwrap();
    s.execute("takeoff.start", &json!({})).unwrap();
    layer(&mut s, "MEASURE-FULLBRICK");
    s.execute("line", &json!({"points": [[0, 0], [9000, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[0, 6000], [9000, 6000]]})).unwrap();
    layer(&mut s, "MEASURE-COLUMN");
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [230, 450]})).unwrap();
    layer(&mut s, "MEASURE-SLAB");
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [9000, 6000]})).unwrap();
    layer(&mut s, "0");
    s.execute("room", &json!({"p1": [100, 100], "p2": [4100, 3100], "name": "br", "units": "mm", "textHeight": 150})).unwrap();
    s.execute("takeoff.brick", &json!({"table": false})).unwrap();
    s.execute("takeoff.columns", &json!({"table": false})).unwrap();
    s.execute("takeoff.slab", &json!({"table": false})).unwrap();
    s
}

/// The PDF the chunks of a database add up to, checked against its header.
fn reassemble(store: &aad_vdb::VdbStore) -> (Vec<u8>, Value) {
    let header = store.records().iter().find(|r| r.id >> 56 == 1).expect("a header");
    let header: Value = serde_json::from_slice(&header.payload).unwrap();
    let pdf = &header["annotated_pdf"];
    let mut chunks: Vec<_> = store.records().iter().filter(|r| r.id >> 56 == 9).collect();
    chunks.sort_by_key(|r| r.id);
    assert_eq!(pdf["chunks"].as_u64(), Some(chunks.len() as u64), "{header}");
    let bytes: Vec<u8> = chunks.iter().flat_map(|c| c.payload.clone()).collect();
    assert_eq!(pdf["bytes"].as_u64(), Some(bytes.len() as u64));
    (bytes, header)
}

#[test]
fn the_real_plotter_makes_a_pdf_that_travels_in_chunks_and_reassembles() {
    let mut s = house();
    let dir = std::env::temp_dir().join(format!("aad-takeoff-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("house.takeoff.vdb");
    let r = s.execute("takeoff.record", &json!({"path": path.to_string_lossy()})).unwrap();
    assert!(r["warnings"].as_array().unwrap().iter().all(|w| !w.as_str().unwrap().starts_with("no annotated drawing")), "{r}");
    let store = aad_vdb::load_path(&path).unwrap();
    let (bytes, header) = reassemble(&store);
    assert!(bytes.starts_with(b"%PDF"), "a PDF");
    assert!(bytes.len() > 1000, "{} bytes", bytes.len());
    assert_eq!(header["state"], "complete");
    assert_eq!(header["annotated_pdf"]["sha256"].as_str().unwrap(), aad_engine::revision::sha256_hex(&bytes));
    assert_eq!(header["annotated_pdf"]["pages"], 1);

    // The same PDF is what `takeoff.annotate` writes.
    let file = dir.join("house.pdf");
    let a = s.execute("takeoff.annotate", &json!({"path": file.to_string_lossy()})).unwrap();
    assert!(a["drawn"].as_u64().unwrap() >= 5, "{a}");
    assert!(std::fs::read(&file).unwrap().starts_with(b"%PDF"));
    std::fs::remove_dir_all(&dir).ok();
}

/// Against a real `shilpid`: `AAD_SHILPID_ADDR=127.0.0.1:7433 cargo test -p aad-cli --test takeoff_pdf -- --ignored`
/// (start it with `shilpid --bind 127.0.0.1:7433 --data x.vdb`). Records two drawings, the second linked to a project.
#[test]
#[ignore = "needs a running shilpid; see the doc comment"]
fn a_running_shilpid_holds_two_drawings_and_a_real_pdf() {
    let Ok(addr) = std::env::var("AAD_SHILPID_ADDR") else { return };
    let mut a = house();
    let ra = a.execute("takeoff.record", &json!({"server": addr})).unwrap();
    let mut b = house();
    let rb = b.execute("takeoff.record", &json!({"server": addr})).unwrap();
    assert_ne!(ra["slot"], rb["slot"], "two drawings, two slots: {ra} {rb}");
    let mut client = aad_vdb::VdbClient::connect(addr.as_str()).unwrap();
    let store = client.pull_store().unwrap();
    assert_eq!(store.records().iter().filter(|r| r.id >> 56 == 1).count(), 2, "two headers");
    for slot in [ra["slot"].as_u64().unwrap(), rb["slot"].as_u64().unwrap()] {
        let mine: Vec<_> = store.records().iter().filter(|r| (r.id >> 32) & 0xFF_FFFF == slot).cloned().collect();
        let (bytes, header) = reassemble(&aad_vdb::VdbStore::from_records(mine).unwrap());
        assert!(bytes.starts_with(b"%PDF"));
        assert_eq!(header["state"], "complete");
    }
}
