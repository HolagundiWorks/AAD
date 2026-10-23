//! Far-zoom LOD and model frustum cull cut display-list work when zoomed in.

use aad_doc::{Circle, Common, Drawing, EntityKind, Hatch, HatchLoop, Space};
use aad_geom::{PolyVertex, Vec2, Vec3};
use aad_render::{Options, build, lod_for_view};

fn hatch_and_circles() -> Drawing {
    let mut d = Drawing::new_metric();
    for i in 0..20 {
        d.add(
            &Space::Model,
            Common::default(),
            EntityKind::Circle(Circle { center: Vec3::new(f64::from(i) * 50.0, 0.0, 0.0), radius: 5.0 }),
        )
        .unwrap();
    }
    let sq = vec![
        PolyVertex::new(Vec2::new(-10.0, -10.0)),
        PolyVertex::new(Vec2::new(10.0, -10.0)),
        PolyVertex::new(Vec2::new(10.0, 10.0)),
        PolyVertex::new(Vec2::new(-10.0, 10.0)),
    ];
    let hatch = Hatch {
        pattern: "ANSI31".into(),
        solid: false,
        loops: vec![HatchLoop { vertices: sq, outer: true }],
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
    };
    d.add(&Space::Model, Common::default(), EntityKind::Hatch(hatch)).unwrap();
    d
}

#[test]
fn lod_turns_off_text_and_fill_when_zoomed_out() {
    let (text, fill) = lod_for_view(100.0, 1.0); // 100 world units per pixel
    assert!(!text && !fill);
    let (text, fill) = lod_for_view(0.01, 1.0); // zoomed in
    assert!(text && fill);
}

#[test]
fn fill_off_skips_hatch_pattern_lines() {
    let d = hatch_and_circles();
    let with_fill = build(
        &d,
        &Space::Model,
        &Options { text: true, fill: true, view_height: 40.0, tolerance: 0.01, ..Default::default() },
    );
    let no_fill = build(
        &d,
        &Space::Model,
        &Options { text: false, fill: false, view_height: 40.0, tolerance: 0.01, ..Default::default() },
    );
    assert!(
        no_fill.prims.len() < with_fill.prims.len(),
        "no_fill={} with_fill={}",
        no_fill.prims.len(),
        with_fill.prims.len()
    );
}

#[test]
fn frustum_skips_entities_outside_the_view() {
    let d = hatch_and_circles();
    let full = build(&d, &Space::Model, &Options { view_height: 2000.0, ..Default::default() });
    let corner = build(
        &d,
        &Space::Model,
        &Options {
            view_height: 30.0,
            view_center: Some(Vec2::new(900.0, 0.0)),
            ..Default::default()
        },
    );
    assert!(corner.prims.len() < full.prims.len(), "corner={} full={}", corner.prims.len(), full.prims.len());
}
