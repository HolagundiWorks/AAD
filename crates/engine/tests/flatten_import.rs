//! Opening a drawing flattens it to 2D on a working copy: the original stays recoverable (undo,
//! `.bak` before the first overwrite), and what could not be flattened is reported.

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, base64_encode, set_io};
use cadcraft_engine::doc::{Circle, Common, Drawing, Entity, EntityKind, Handle, Line, LwPolyline, Space, Unknown};
use cadcraft_engine::geom::{PolyVertex, Vec2, Vec3};
use serde_json::json;

fn raw(d: &mut Drawing, common: Common, kind: EntityKind) -> Handle {
    let h = d.new_handle();
    d.space_mut(&Space::Model).unwrap().push(Entity { handle: h, common, kind });
    h
}

/// `solid.dxf` carries 3D data; any other name is a plain 2D drawing.
fn read(_: &[u8], name: &str) -> Result<Drawing, String> {
    let mut d = Drawing::new_imperial();
    raw(&mut d, Common::default(), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 5.0), b: Vec3::new(4.0, 0.0, 9.0) }));
    if name.contains("solid") {
        raw(&mut d, Common { thickness: 3.0, ..Common::default() }, EntityKind::Circle(Circle { center: Vec3::new(1.0, 1.0, 2.0), radius: 1.0 }));
        let poly = LwPolyline {
            vertices: vec![PolyVertex::new(Vec2::ZERO), PolyVertex::new(Vec2::new(1.0, 0.0)), PolyVertex::new(Vec2::new(1.0, 1.0))],
            closed: true,
            const_width: 0.0,
            elevation: 12.0,
            plinegen: false,
        };
        raw(&mut d, Common::default(), EntityKind::LwPolyline(poly));
        raw(&mut d, Common::default(), EntityKind::Unknown(Unknown { dxf_type: "3DSOLID".into(), tags: Vec::new() }));
        raw(&mut d, Common::default(), EntityKind::Unknown(Unknown { dxf_type: "3DSOLID".into(), tags: Vec::new() }));
        raw(&mut d, Common::default(), EntityKind::Unknown(Unknown { dxf_type: "MESH".into(), tags: Vec::new() }));
        raw(&mut d, Common::default(), EntityKind::Unknown(Unknown { dxf_type: "ACAD_TABLE".into(), tags: Vec::new() }));
    } else {
        // A 2D drawing: the line is flat.
        d = Drawing::new_imperial();
        raw(&mut d, Common::default(), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(4.0, 0.0, 0.0) }));
    }
    Ok(d)
}

fn install() {
    set_io(IoHooks { read, write: |_, _| Ok(b"saved".to_vec()), plot: None });
}

fn zs(s: &Session) -> Vec<f64> {
    s.doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Line(l) => Some(l.a.z + l.b.z),
            EntityKind::Circle(c) => Some(c.center.z),
            EntityKind::LwPolyline(p) => Some(p.elevation),
            _ => None,
        })
        .collect()
}

#[test]
fn open_flattens_a_working_copy_and_reports_what_it_could_not() {
    install();
    let mut s = Session::new();
    let r = s.execute("open", &json!({"data": base64_encode(b"x"), "name": "solid.dxf"})).unwrap();
    let f = &r["flatten"];
    assert_eq!(f["flattened"], 3, "line, circle and polyline: {f:#}");
    assert_eq!(f["unsupported3d"], json!({"3DSOLID": 2, "MESH": 1}));
    assert_eq!(f["unmodelled"], json!({"ACAD_TABLE": 1}));
    assert!(zs(&s).iter().all(|z| *z == 0.0), "{:?}", zs(&s));
    assert!(s.doc().unwrap().model.iter().all(|e| e.common.thickness == 0.0));
    assert!(s.log.iter().any(|l| l.contains("Could not flatten") && l.contains("2 3DSOLID")), "{:?}", s.log);

    // The drawing as read is the saved state and one undo step: flattening can be reverted.
    let st = s.state().unwrap();
    assert!(st.is_dirty());
    assert_eq!(st.undo.last().map(|u| u.label.as_str()), Some("Flatten on import"));
    s.execute("u", &json!({})).unwrap();
    let mut z = zs(&s);
    z.sort_by(f64::total_cmp);
    assert_eq!(z, vec![2.0, 12.0, 14.0], "original elevations are back");
}

#[test]
fn a_flat_drawing_opens_unchanged_and_clean() {
    install();
    let mut s = Session::new();
    let r = s.execute("open", &json!({"data": base64_encode(b"x"), "name": "plan.dxf"})).unwrap();
    assert_eq!(r["flatten"]["flattened"], 0);
    let st = s.state().unwrap();
    assert!(!st.is_dirty(), "nothing changed, nothing to save");
    assert!(st.undo.is_empty());
    assert!(!s.log.iter().any(|l| l.contains("Flatten on import")), "{:?}", s.log);
}

#[test]
fn the_first_overwrite_keeps_the_original_file_as_bak() {
    install();
    let dir = std::env::temp_dir().join(format!("cadcraft-bak-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plan.dxf");
    std::fs::write(&path, b"original bytes").unwrap();
    let mut s = Session::new();
    s.execute("open", &json!({"path": path.to_str().unwrap()})).unwrap();
    s.execute("qsave", &json!({})).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"saved");
    let bak = dir.join("plan.dxf.bak");
    assert_eq!(std::fs::read(&bak).unwrap(), b"original bytes");
    // A later save doesn't replace the backup of the original.
    s.execute("qsave", &json!({})).unwrap();
    assert_eq!(std::fs::read(&bak).unwrap(), b"original bytes");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_drawing_without_known_units_is_reported_not_guessed() {
    install();
    let mut s = Session::new();
    s.execute("open", &json!({"data": base64_encode(b"x"), "name": "plan.dxf"})).unwrap();
    let unknown = cadcraft_engine::annot::drawing_unit(s.doc().unwrap()).is_none();
    assert_eq!(unknown, s.log.iter().any(|l| l.contains("units are not set")), "{:?}", s.log);
    // Setting the unit is explicit, and the geometry is untouched either way.
    s.execute("setvar", &json!({"name": "INSUNITS", "value": 0})).unwrap();
    assert!(s.execute("display.units", &json!({"units": "m"})).is_err());
}
