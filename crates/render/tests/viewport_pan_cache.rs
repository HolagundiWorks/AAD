//! Panning a layout viewport reuses tessellated model content (transform + clip only).

use aad_doc::{Circle, Common, Drawing, EntityKind, Space, Viewport};
use aad_geom::{Vec2, Vec3};
use aad_render::{Options, VIEWPORT_CONTENT, build};

fn layout_with_model() -> Drawing {
    let mut d = Drawing::new_metric();
    for i in 0..40 {
        let x = f64::from(i % 8) * 10.0;
        let y = f64::from(i / 8) * 10.0;
        d.add(&Space::Model, Common::default(), EntityKind::Circle(Circle { center: Vec3::new(x, y, 0.0), radius: 2.0 })).unwrap();
    }
    let vp = Viewport {
        center: Vec3::new(50.0, 40.0, 0.0),
        width: 80.0,
        height: 60.0,
        view_center: Vec2::new(20.0, 20.0),
        view_height: 50.0,
        id: 2,
        ..Default::default()
    };
    d.add(&Space::Paper("Layout1".into()), Common::default(), EntityKind::Viewport(vp)).unwrap();
    d
}

fn vp_handle(d: &Drawing) -> aad_doc::Handle {
    d.layouts[0]
        .entities
        .iter()
        .find(|e| matches!(e.kind, EntityKind::Viewport(_)))
        .map(|e| e.handle)
        .expect("viewport")
}

fn content_count(list: &aad_render::DisplayList) -> usize {
    list.prims.iter().filter(|p| p.handle == VIEWPORT_CONTENT).count()
}

fn content_centroid(list: &aad_render::DisplayList) -> Vec2 {
    let mut n = 0usize;
    let mut s = Vec2::ZERO;
    for p in list.prims.iter().filter(|p| p.handle == VIEWPORT_CONTENT) {
        for q in list.points(p) {
            s = s + *q;
            n += 1;
        }
    }
    if n == 0 { Vec2::ZERO } else { s * (1.0 / n as f64) }
}

#[test]
fn panning_the_viewport_keeps_model_content_and_changes_the_clip() {
    let mut d = layout_with_model();
    let space = Space::Paper("Layout1".into());
    let opts = Options::default();
    let a = build(&d, &space, &opts);
    let before = content_count(&a);
    assert!(before > 0, "viewport shows model content");

    // Fine pan: stays well inside the coarse cache cell (8 × view_height).
    let h = vp_handle(&d);
    d.modify_entity(h, |e| {
        if let EntityKind::Viewport(vp) = &mut e.kind {
            vp.view_center.x += 1.0;
            vp.view_center.y += 0.5;
        }
    })
    .unwrap();
    let b = build(&d, &space, &opts);
    assert_eq!(content_count(&b), before, "pan must not drop model primitives");
    assert!(content_centroid(&a).dist(content_centroid(&b)) > 0.1, "model content shifts on pan");
}

#[test]
fn a_larger_pan_within_the_cell_still_keeps_model_content() {
    let mut d = layout_with_model();
    let space = Space::Paper("Layout1".into());
    let opts = Options::default();
    let a = build(&d, &space, &opts);
    let before = content_count(&a);
    // view_height=50 → cell=400; Δcenter=25 stays over the drawing and in the same cell.
    let h = vp_handle(&d);
    d.modify_entity(h, |e| {
        if let EntityKind::Viewport(vp) = &mut e.kind {
            vp.view_center.x += 25.0;
        }
    })
    .unwrap();
    let b = build(&d, &space, &opts);
    assert!(content_count(&b) > 0 && before > 0, "viewport still shows model content");
    // Clip membership can change as the window moves; the important part is content still draws.
    assert!(content_centroid(&a).dist(content_centroid(&b)) > 1.0, "model content shifts on a larger pan");
}

#[test]
fn crossing_a_cell_boundary_and_returning_keeps_content() {
    let mut d = layout_with_model();
    let space = Space::Paper("Layout1".into());
    let opts = Options::default();
    let a = build(&d, &space, &opts);
    let before = content_count(&a);
    let h = vp_handle(&d);
    // Jump more than half a cell (cell = 400) so the pan_cell index changes, then jump back.
    // The mid view may be empty of model geometry; the LRU must still restore the first cell.
    d.modify_entity(h, |e| {
        if let EntityKind::Viewport(vp) = &mut e.kind {
            vp.view_center.x += 500.0;
        }
    })
    .unwrap();
    let _mid = build(&d, &space, &opts);
    d.modify_entity(h, |e| {
        if let EntityKind::Viewport(vp) = &mut e.kind {
            vp.view_center.x -= 500.0;
        }
    })
    .unwrap();
    let back = build(&d, &space, &opts);
    assert_eq!(content_count(&back), before, "returning to the first cell must still show model content");
}
