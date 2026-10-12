//! A big model space is built in parallel chunks; the list must be what one thread would build, in
//! draw order.

use cadcraft_doc::{Common, Drawing, Entity, EntityKind, Line, Space};
use cadcraft_geom::Vec3;
use cadcraft_render::{Options, build, build_entities};

fn opts() -> Options {
    Options { tolerance: 0.01, min_dash: 0.1, text: true, fill: true, lineweights: false, view_height: 100.0, plot_style_table: None }
}

#[test]
fn the_parallel_build_matches_building_entity_by_entity() {
    let mut d = Drawing::new_imperial();
    for i in 0..9000 {
        let y = f64::from(i);
        let kind = EntityKind::Line(Line { a: Vec3::new(y, 0.0, 0.0), b: Vec3::new(y + 1.0, y, 0.0) });
        d.add(&Space::Model, Common::default(), kind).unwrap();
    }
    let ents: Vec<&Entity> = d.model.iter().map(|e| &**e).collect();
    let whole = build(&d, &Space::Model, &opts());
    let serial = build_entities(&d, ents, &opts());
    assert_eq!(whole.prims.len(), 9000);
    assert_eq!(whole.prims.len(), serial.prims.len());
    assert_eq!(whole.verts, serial.verts, "same points in the same order");
    for (a, b) in whole.prims.iter().zip(&serial.prims) {
        assert_eq!((a.handle, a.start, a.len, a.lw), (b.handle, b.start, b.len, b.lw));
    }
    assert_eq!(whole.bounds, serial.bounds);
}

fn same_list(a: &cadcraft_render::DisplayList, b: &cadcraft_render::DisplayList) {
    assert_eq!(a.prims, b.prims);
    assert_eq!(a.verts, b.verts);
    assert_eq!(a.tris, b.tris);
    assert_eq!(a.bounds, b.bounds);
}

#[test]
fn an_incremental_rebuild_equals_a_full_build_after_edits() {
    let mut old = Drawing::new_imperial();
    let mut handles = Vec::new();
    for i in 0..6000 {
        let y = f64::from(i);
        let kind = EntityKind::Line(Line { a: Vec3::new(y, 0.0, 0.0), b: Vec3::new(y + 1.0, y, 0.0) });
        handles.push(old.add(&Space::Model, Common::default(), kind).unwrap());
    }
    let before = build(&old, &Space::Model, &opts());
    let mut new = old.clone();
    new.model.remove(handles[10]);
    new.model.remove(handles[11]);
    new.model.remove(handles[5999]);
    new.model.modify(handles[3000], |e| {
        if let EntityKind::Line(l) = &mut e.kind {
            l.b = Vec3::new(9.0e4, 5.0, 0.0);
        }
    });
    new.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(-50.0, 7.0, 0.0) })).unwrap();
    let inc = cadcraft_render::rebuild(&before, &old, &new, &opts()).expect("the edit is incremental");
    same_list(&inc, &build(&new, &Space::Model, &opts()));
    // A change to a layer is not an entity edit: it cannot be done incrementally.
    let mut layers = new.clone();
    layers.layers.clear();
    assert!(cadcraft_render::rebuild(&inc, &new, &layers, &opts()).is_none());
}
