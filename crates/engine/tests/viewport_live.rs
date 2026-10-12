//! Panning and zooming inside an MSPACE viewport changes what the viewport shows, so the drawing
//! must be redrawn at once (the canvas rebuilds its display list when the revision moves).

use aad_engine::geom::Vec2;
use aad_engine::{Session, View};
use serde_json::json;

fn viewport_session() -> Session {
    let mut s = Session::new();
    s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    s.execute("mview", &json!({"count": 1})).unwrap();
    s.execute("mspace", &json!({})).unwrap();
    s
}

fn view_center(s: &Session) -> Vec2 {
    s.state().unwrap().active_viewport().map(|(_, v)| v.view_center).unwrap()
}

#[test]
fn realtime_pan_and_zoom_in_a_viewport_bump_the_revision() {
    let mut s = viewport_session();
    let (before, rev) = (view_center(&s), s.state().unwrap().revision);
    let v = s.state().unwrap().view();
    s.state_mut().unwrap().set_view_quiet(View { center: v.center + Vec2::new(0.5, 0.25), height: v.height });
    assert!(view_center(&s) != before, "the viewport's model view moved");
    assert!(s.state().unwrap().revision > rev, "so the drawing is redrawn");

    let rev = s.state().unwrap().revision;
    let v = s.state().unwrap().view();
    s.state_mut().unwrap().set_view_quiet(View { center: v.center, height: v.height * 0.5 });
    assert!(s.state().unwrap().revision > rev, "zooming too");

    // On the sheet itself nothing in the drawing changes, so there is nothing to redraw.
    s.execute("pspace", &json!({})).unwrap();
    let rev = s.state().unwrap().revision;
    let v = s.state().unwrap().view();
    s.state_mut().unwrap().set_view_quiet(View { center: v.center + Vec2::new(1.0, 0.0), height: v.height });
    assert_eq!(s.state().unwrap().revision, rev);
}
