//! Seed standard place-tool block definitions (door / window / furniture / trees / …).
//!
//! Geometry is drawn in **metres** about the insertion point. Place with INSERT
//! scale to resize; each block carries a `WIDTH` param (metres) for takeoff.

use aadt_geometry::{Circle, Point2, Polyline, Segment};
use aadt_model::{Block, BlockParamKind, Document, Geometry};

/// Ensure the named place-tool blocks exist so INSERT succeeds.
pub fn ensure_place_blocks(doc: &mut Document) {
    seed_door_single(doc);
    seed_door_double(doc);
    seed_window(doc);
    seed_window_sliding(doc);
    seed_column(doc);
    seed_beam(doc);
    seed_slab(doc);
    seed_tree(doc);
    seed_shrub(doc);
    seed_chair(doc);
    seed_desk(doc);
    seed_sofa(doc);
    seed_bed(doc);
    seed_table(doc);
    seed_toilet(doc);
}

fn seed_door_single(doc: &mut Document) {
    seed(doc, "DOOR-SINGLE", 0.9, |b| {
        b.push(
            "A-DOOR",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(0.9, 0.0))),
        );
        b.push(
            "A-DOOR",
            Geometry::Line(Segment::new(Point2::new(0.9, 0.0), Point2::new(0.9, 0.05))),
        );
        b.push(
            "A-DOOR",
            Geometry::Arc(aadt_geometry::Arc::new(
                Point2::new(0.0, 0.0),
                0.9,
                0.0,
                std::f64::consts::FRAC_PI_2,
            )),
        );
    });
}

fn seed_door_double(doc: &mut Document) {
    seed(doc, "DOOR-DOUBLE", 1.8, |b| {
        b.push(
            "A-DOOR",
            Geometry::Line(Segment::new(Point2::new(-0.9, 0.0), Point2::new(0.9, 0.0))),
        );
        b.push(
            "A-DOOR",
            Geometry::Arc(aadt_geometry::Arc::new(
                Point2::new(-0.9, 0.0),
                0.9,
                0.0,
                std::f64::consts::FRAC_PI_2,
            )),
        );
        b.push(
            "A-DOOR",
            Geometry::Arc(aadt_geometry::Arc::new(
                Point2::new(0.9, 0.0),
                0.9,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
            )),
        );
    });
}

fn seed_window(doc: &mut Document) {
    seed(doc, "WINDOW", 1.2, |b| {
        b.push(
            "A-GLAZ",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(1.2, 0.0),
                    Point2::new(1.2, 0.1),
                    Point2::new(0.0, 0.1),
                ],
                true,
            )),
        );
        b.push(
            "A-GLAZ",
            Geometry::Line(Segment::new(Point2::new(0.6, 0.0), Point2::new(0.6, 0.1))),
        );
    });
}

fn seed_window_sliding(doc: &mut Document) {
    seed(doc, "WINDOW-SLIDER", 1.8, |b| {
        b.push(
            "A-GLAZ",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(1.8, 0.0),
                    Point2::new(1.8, 0.12),
                    Point2::new(0.0, 0.12),
                ],
                true,
            )),
        );
        b.push(
            "A-GLAZ",
            Geometry::Line(Segment::new(Point2::new(0.9, 0.0), Point2::new(0.9, 0.12))),
        );
        b.push(
            "A-GLAZ",
            Geometry::Line(Segment::new(
                Point2::new(0.15, 0.06),
                Point2::new(0.75, 0.06),
            )),
        );
    });
}

fn seed_column(doc: &mut Document) {
    seed(doc, "COLUMN", 0.4, |b| {
        b.push(
            "S-COLS",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.2)),
        );
    });
}

fn seed_beam(doc: &mut Document) {
    seed(doc, "BEAM", 2.0, |b| {
        b.push(
            "S-BEAM",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, -0.1),
                    Point2::new(2.0, -0.1),
                    Point2::new(2.0, 0.1),
                    Point2::new(0.0, 0.1),
                ],
                true,
            )),
        );
    });
}

fn seed_slab(doc: &mut Document) {
    seed(doc, "SLAB", 3.0, |b| {
        b.push(
            "A-FLOR",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(3.0, 0.0),
                    Point2::new(3.0, 2.0),
                    Point2::new(0.0, 2.0),
                ],
                true,
            )),
        );
    });
}

fn seed_tree(doc: &mut Document) {
    seed(doc, "TREE", 4.0, |b| {
        // Canopy (plan) + trunk mark — typical 4 m canopy diameter.
        b.push(
            "S-TREE",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 2.0)),
        );
        b.push(
            "S-TREE",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 1.4)),
        );
        b.push(
            "S-TREE",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.15)),
        );
    });
}

fn seed_shrub(doc: &mut Document) {
    seed(doc, "SHRUB", 1.2, |b| {
        b.push(
            "S-TREE",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.6)),
        );
        for i in 0..6 {
            let a = (i as f64) * std::f64::consts::TAU / 6.0;
            let p = Point2::new(0.55 * a.cos(), 0.55 * a.sin());
            b.push(
                "S-TREE",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), p)),
            );
        }
    });
}

fn seed_chair(doc: &mut Document) {
    seed(doc, "FURN-CHAIR", 0.5, |b| {
        b.push(
            "A-FURN",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(-0.25, -0.25),
                    Point2::new(0.25, -0.25),
                    Point2::new(0.25, 0.25),
                    Point2::new(-0.25, 0.25),
                ],
                true,
            )),
        );
        b.push(
            "A-FURN",
            Geometry::Line(Segment::new(
                Point2::new(-0.25, 0.25),
                Point2::new(0.25, 0.25),
            )),
        );
    });
}

fn seed_desk(doc: &mut Document) {
    seed(doc, "FURN-DESK", 1.5, |b| {
        b.push(
            "A-FURN",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(1.5, 0.0),
                    Point2::new(1.5, 0.75),
                    Point2::new(0.0, 0.75),
                ],
                true,
            )),
        );
    });
}

fn seed_sofa(doc: &mut Document) {
    seed(doc, "FURN-SOFA", 2.0, |b| {
        b.push(
            "A-FURN",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(2.0, 0.0),
                    Point2::new(2.0, 0.85),
                    Point2::new(0.0, 0.85),
                ],
                true,
            )),
        );
        b.push(
            "A-FURN",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.7), Point2::new(2.0, 0.7))),
        );
    });
}

fn seed_bed(doc: &mut Document) {
    seed(doc, "FURN-BED", 2.0, |b| {
        b.push(
            "A-FURN",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(2.0, 0.0),
                    Point2::new(2.0, 1.6),
                    Point2::new(0.0, 1.6),
                ],
                true,
            )),
        );
        b.push(
            "A-FURN",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.4), Point2::new(2.0, 0.4))),
        );
    });
}

fn seed_table(doc: &mut Document) {
    seed(doc, "FURN-TABLE", 1.2, |b| {
        b.push(
            "A-FURN",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.0), 0.6)),
        );
    });
}

fn seed_toilet(doc: &mut Document) {
    seed(doc, "FIXTURE-TOILET", 0.4, |b| {
        b.push(
            "A-FLOR",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(-0.2, 0.0),
                    Point2::new(0.2, 0.0),
                    Point2::new(0.2, 0.55),
                    Point2::new(-0.2, 0.55),
                ],
                true,
            )),
        );
        b.push(
            "A-FLOR",
            Geometry::Circle(Circle::new(Point2::new(0.0, 0.28), 0.12)),
        );
    });
}

fn seed(doc: &mut Document, name: &str, width_m: f64, f: impl FnOnce(&mut Block)) {
    if doc.block(name).is_some() {
        return;
    }
    let mut block = Block::new(name, Point2::new(0.0, 0.0));
    f(&mut block);
    block.set_param("WIDTH", format!("{width_m:.3}"), BlockParamKind::Number);
    block.set_param("UNITS", "m", BlockParamKind::String);
    doc.add_block(block);
}
