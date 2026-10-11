//! Property-based tests of the geometry kernel. Each property is an invariant that must hold for
//! *any* valid input in its range, so these find the cases nobody thought to write down. Numbers are
//! compared with a tolerance that scales with the size of the inputs, never with `==`.
//!
//! A failing case is shrunk to a small one and saved in `proptest-regressions/` (commit it: it
//! becomes a permanent regression test). `PROPTEST_CASES` raises the number of cases per property
//! (the scheduled job does).

use cadcraft_geom::{
    Arc, Bounds2, Circle, Line, Mat3, Polyline, Segment, Vec2, bulge_to_arc, circle_circle, intersect_segments, line_circle, line_line, shoelace,
};
use proptest::prelude::*;

const R: f64 = 1.0e6;

fn coord() -> impl Strategy<Value = f64> {
    -R..R
}

fn pt() -> impl Strategy<Value = Vec2> {
    (coord(), coord()).prop_map(|(x, y)| Vec2::new(x, y))
}

/// Tolerance for a quantity computed from values of magnitude `scale`.
fn tol(scale: f64) -> f64 {
    1e-9 * (1.0 + scale)
}

fn close(a: f64, b: f64, scale: f64) -> bool {
    (a - b).abs() <= tol(scale)
}

proptest! {

    // ---- transformations keep shape ----

    #[test]
    fn translating_a_line_keeps_its_length(a in pt(), b in pt(), d in pt()) {
        let l = Line::new(a, b);
        let m = Line::new(a + d, b + d);
        prop_assert!(close(l.len(), m.len(), l.len() + d.len()));
    }

    #[test]
    fn rotating_about_a_point_keeps_distances(a in pt(), b in pt(), c in pt(), angle in -7.0f64..7.0) {
        let (a2, b2) = (a.rotate_about(c, angle), b.rotate_about(c, angle));
        prop_assert!(close(a.dist(b), a2.dist(b2), a.len() + b.len() + c.len()));
    }

    #[test]
    fn a_transform_and_its_inverse_cancel(p in pt(), d in pt(), angle in -3.0f64..3.0, k in 0.01f64..100.0) {
        let m = Mat3::translate(d).then(Mat3::rotate(angle)).then(Mat3::scale(k, k));
        let inv = m.inverse();
        prop_assert!(inv.is_some(), "a non-degenerate transform is invertible");
        if let Some(inv) = inv {
            let back = inv.apply(m.apply(p));
            let scale = p.len() + d.len() * k + 1.0;
            prop_assert!(back.near(p, 1e-9 * scale * (1.0 + k)), "{back:?} vs {p:?}");
        }
    }

    #[test]
    fn mirroring_twice_is_the_identity(p in pt(), a in pt(), b in pt()) {
        prop_assume!(a.dist(b) > 1.0);
        let twice = p.mirror(a, b).mirror(a, b);
        prop_assert!(twice.near(p, 1e-9 * (p.len() + a.len() + b.len() + 1.0)));
    }

    // ---- lines ----

    #[test]
    fn the_closest_point_on_a_line_is_on_it_and_no_farther_than_the_ends(a in pt(), b in pt(), p in pt()) {
        prop_assume!(a.dist(b) > 1e-3);
        let l = Line::new(a, b);
        let c = l.closest(p);
        let scale = a.len() + b.len() + p.len();
        prop_assert!(l.dist(c) <= tol(scale) * 10.0, "closest point is on the segment");
        prop_assert!(p.dist(c) <= p.dist(a) + tol(scale) && p.dist(c) <= p.dist(b) + tol(scale));
    }

    #[test]
    fn line_line_intersections_lie_on_both_lines_and_do_not_depend_on_order(a in pt(), b in pt(), c in pt(), d in pt()) {
        prop_assume!(a.dist(b) > 1.0 && c.dist(d) > 1.0);
        let (l1, l2) = (Line::new(a, b), Line::new(c, d));
        let x = line_line(&l1, &l2);
        let y = line_line(&l2, &l1);
        prop_assert_eq!(x.is_some(), y.is_some(), "symmetric");
        if let (Some(p), Some(q)) = (x, y) {
            let scale = a.len() + b.len() + c.len() + d.len();
            prop_assert!(p.near(q, 1e-6 * (1.0 + scale)), "same point either way");
            // Near-parallel lines amplify rounding; the point must still be close to both segments.
            let sin = (b - a).normalized().cross((d - c).normalized()).abs();
            if sin > 1e-3 {
                prop_assert!(l1.dist(p) <= 1e-6 * (1.0 + scale) && l2.dist(p) <= 1e-6 * (1.0 + scale), "{p:?}");
            }
        }
    }

    // ---- circles ----

    #[test]
    fn circle_circle_points_are_on_both_circles(c1 in pt(), c2 in pt(), r1 in 1.0f64..R, r2 in 1.0f64..R) {
        let (a, b) = (Circle::new(c1, r1), Circle::new(c2, r2));
        for p in circle_circle(&a, &b) {
            let scale = c1.len() + c2.len() + r1 + r2;
            prop_assert!((p.dist(c1) - r1).abs() <= 1e-5 * (1.0 + scale), "on circle 1");
            prop_assert!((p.dist(c2) - r2).abs() <= 1e-5 * (1.0 + scale), "on circle 2");
        }
    }

    #[test]
    fn line_circle_points_are_on_the_circle_and_the_infinite_line(a in pt(), b in pt(), c in pt(), r in 1.0f64..R) {
        prop_assume!(a.dist(b) > 1.0);
        let l = Line::new(a, b);
        let circ = Circle::new(c, r);
        for (p, _t) in line_circle(&l, &circ) {
            let scale = a.len() + b.len() + c.len() + r;
            prop_assert!((p.dist(c) - r).abs() <= 1e-5 * (1.0 + scale), "on the circle");
            // `line_circle` intersects the infinite line through the segment (see its documentation).
            let off_line = (p - a).cross(b - a).abs() / a.dist(b);
            prop_assert!(off_line <= 1e-5 * (1.0 + scale), "on the line through the segment: {off_line}");
        }
    }

    // ---- polygons and polylines ----

    #[test]
    fn polygon_area_ignores_position_and_rotation_and_flips_sign_when_reversed(
        pts in prop::collection::vec(pt(), 3..12), d in pt(), angle in -3.0f64..3.0
    ) {
        let a = shoelace(&pts);
        let scale = pts.iter().map(|p| p.len()).fold(0.0, f64::max);
        let area_tol = 1e-7 * (1.0 + scale * scale);
        let moved: Vec<Vec2> = pts.iter().map(|p| *p + d).collect();
        prop_assert!((shoelace(&moved) - a).abs() <= area_tol * (1.0 + d.len() / (1.0 + scale)), "translation");
        let turned: Vec<Vec2> = pts.iter().map(|p| p.rotate(angle)).collect();
        prop_assert!((shoelace(&turned) - a).abs() <= area_tol, "rotation");
        let mut rev = pts.clone();
        rev.reverse();
        prop_assert!((shoelace(&rev) + a).abs() <= area_tol, "reversing the order flips the sign");
    }

    #[test]
    fn scaling_a_polyline_scales_its_length_and_area(pts in prop::collection::vec(pt(), 3..10), k in 0.01f64..50.0) {
        let p = Polyline::from_points(&pts, true);
        let q = Polyline::from_points(&pts.iter().map(|v| *v * k).collect::<Vec<_>>(), true);
        let scale = pts.iter().map(|v| v.len()).fold(0.0, f64::max);
        prop_assert!((q.len() - p.len() * k).abs() <= 1e-7 * (1.0 + p.len() * k));
        prop_assert!((q.area() - p.area() * k * k).abs() <= 1e-7 * (1.0 + scale * scale * k * k * 10.0));
    }

    #[test]
    fn bounds_contain_the_points_they_were_made_from(pts in prop::collection::vec(pt(), 1..30)) {
        let b = Bounds2::from_points(pts.iter().copied());
        for p in &pts {
            prop_assert!(b.contains(*p));
        }
        let other = Bounds2::from_points([Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)]);
        let u = b.union(&other);
        prop_assert!(u.contains_box(&b) && u.contains_box(&other));
    }

    // ---- arcs and bulges ----

    #[test]
    fn a_bulge_arc_runs_between_its_two_points(a in pt(), b in pt(), bulge in -5.0f64..5.0) {
        prop_assume!(a.dist(b) > 1.0 && bulge.abs() > 1e-3);
        if let Some((arc, _ccw)) = bulge_to_arc(a, b, bulge) {
            let scale = a.len() + b.len() + arc.radius;
            let ends = [arc.start_point(), arc.end_point()];
            let t = 1e-5 * (1.0 + scale);
            let ok = (ends[0].near(a, t) && ends[1].near(b, t)) || (ends[0].near(b, t) && ends[1].near(a, t));
            prop_assert!(ok, "{ends:?} vs {a:?}, {b:?}");
            prop_assert!(arc.len() >= a.dist(b) - 1e-6 * (1.0 + scale), "an arc is never shorter than its chord");
        }
    }

    #[test]
    fn three_points_make_an_arc_through_them(p1 in pt(), p2 in pt(), p3 in pt()) {
        if let Some(arc) = Arc::from_3_points(p1, p2, p3) {
            let scale = p1.len() + p2.len() + p3.len();
            for p in [p1, p2, p3] {
                // Almost collinear points give a huge radius: only well-conditioned arcs are checked.
                if arc.radius < 1e5 {
                    prop_assert!((p.dist(arc.center) - arc.radius).abs() <= 1e-4 * (1.0 + scale), "{p:?}");
                }
            }
        }
    }

    // ---- hostile numbers: nothing panics, whatever the values ----

    #[test]
    fn geometry_functions_survive_any_float_including_nan_and_infinity(
        x1 in prop::num::f64::ANY, y1 in prop::num::f64::ANY, x2 in prop::num::f64::ANY, y2 in prop::num::f64::ANY,
        x3 in prop::num::f64::ANY, y3 in prop::num::f64::ANY, r in prop::num::f64::ANY, bulge in prop::num::f64::ANY
    ) {
        let (a, b, c) = (Vec2::new(x1, y1), Vec2::new(x2, y2), Vec2::new(x3, y3));
        let (l1, l2) = (Line::new(a, b), Line::new(b, c));
        let _ = (l1.len(), l1.closest(c), l1.dist(c), l1.offset(r), l1.dir(), l1.mid());
        let _ = line_line(&l1, &l2);
        let (c1, c2) = (Circle::new(a, r), Circle::new(b, r));
        let _ = (circle_circle(&c1, &c2), line_circle(&l1, &c1), c1.tangent_points(c), c1.common_tangents(&c2), c1.bounds());
        let _ = (Arc::from_3_points(a, b, c), Circle::from_3_points(a, b, c), bulge_to_arc(a, b, bulge));
        let poly = Polyline::from_points(&[a, b, c], true);
        let _ = (poly.area(), poly.len(), poly.bounds(), poly.closest(a), shoelace(&[a, b, c]));
        let s1 = Segment::Line(l1);
        let s2 = Segment::Line(l2);
        let _ = intersect_segments(&s1, &s2);
        let m = Mat3::translate(a).then(Mat3::rotate(r)).then(Mat3::scale(x3, y3));
        let _ = (m.inverse(), m.apply(b), m.det());
    }
}
