//! A new session starts without grid display or grid snap, keeps object snap on, and does not
//! quantize typed coordinates.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::json;

#[test]
fn grid_and_grid_snap_start_off_and_coordinates_are_not_quantized() {
    let mut s = Session::new();
    assert!(!s.settings.gridmode, "grid display is off by default");
    assert!(!s.settings.snapmode, "grid snap is off by default");
    assert!(s.settings.osmode != 0 && s.settings.osmode & cadcraft_engine::snap::mode::OFF == 0, "object snap stays on");
    s.cmdline("line 0.37,0.91 3.33,4.07").unwrap();
    s.cmdline("").unwrap();
    let d = s.doc().unwrap();
    let EntityKind::Line(l) = &d.model.iter().next().unwrap().kind else { panic!("expected a line") };
    assert_eq!((l.a.x, l.a.y, l.b.x, l.b.y), (0.37, 0.91, 3.33, 4.07));
    // Both stay available as options.
    s.execute("grid", &json!({"on": true})).unwrap();
    s.execute("snap", &json!({"on": true})).unwrap();
    assert!(s.settings.gridmode && s.settings.snapmode);
}
