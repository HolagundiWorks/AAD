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
