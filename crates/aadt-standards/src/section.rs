//! Plan cut-line → 2D section drawing from AIA wall / opening layers.

use aadt_geometry::{Point2, Polyline, Segment};
use aadt_model::{Document, Geometry};

/// Generate a 2D section on `A-SECT*` layers from a plan cut line.
///
/// Heuristic v0: wall segments on `A-WALL*` that intersect the cut become
/// vertical rectangles of height `storey_height` (default 3.0 drawing units).
/// Openings on `A-DOOR*` / `A-GLAZ*` that hit the cut leave gaps (no fill).
/// Geometry is placed with X = distance along the cut and Y = height.
pub fn generate_section(
    doc: &mut Document,
    cut_a: Point2,
    cut_b: Point2,
    storey_height: f64,
) -> Result<usize, String> {
    let cut = Segment::new(cut_a, cut_b);
    let cut_len = cut_a.distance(cut_b);
    if cut_len < 1e-9 {
        return Err("SECTION: cut line is degenerate".into());
    }
    if storey_height <= 0.0 {
        return Err("SECTION: storey height must be positive".into());
    }

    let dir = Point2::new(cut_b.x - cut_a.x, cut_b.y - cut_a.y);
    let along =
        |p: Point2| -> f64 { ((p.x - cut_a.x) * dir.x + (p.y - cut_a.y) * dir.y) / cut_len };

    // Collect wall hits and opening gaps along the cut (parametric 0..cut_len).
    let mut walls: Vec<(f64, f64)> = Vec::new(); // (s0, s1) along cut
    let mut openings: Vec<(f64, f64)> = Vec::new();

    let entities: Vec<_> = doc
        .entities()
        .into_iter()
        .map(|e| (e.layer.clone(), e.geometry.clone()))
        .collect();
    for (layer, geometry) in &entities {
        let u = layer.to_ascii_uppercase();
        let is_wall = u.starts_with("A-WALL");
        let is_open =
            u.starts_with("A-DOOR") || u.starts_with("A-GLAZ") || u.starts_with("A-WINDOW");
        if !is_wall && !is_open {
            continue;
        }
        for seg in geometry.segments() {
            if let Some(hit) = segment_intersection(cut, seg) {
                let s = along(hit).clamp(0.0, cut_len);
                // Thickness proxy: project segment length component along cut, min width.
                let half = (seg.a.distance(seg.b) * 0.05).clamp(0.05, 0.25);
                let range = ((s - half).max(0.0), (s + half).min(cut_len));
                if is_wall {
                    walls.push(range);
                } else {
                    openings.push(range);
                }
            } else {
                // Also accept segments whose midpoint is near the cut (crossing tolerance).
                let mid = seg.a.midpoint(seg.b);
                let dist = point_to_segment_dist(mid, cut);
                if dist < 0.15 {
                    let s = along(mid).clamp(0.0, cut_len);
                    let half = (seg.a.distance(seg.b) * 0.5).clamp(0.1, 1.0);
                    let range = ((s - half).max(0.0), (s + half).min(cut_len));
                    if is_wall {
                        walls.push(range);
                    } else {
                        openings.push(range);
                    }
                }
            }
        }
    }

    // Merge overlapping wall intervals, subtract openings.
    walls.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let merged = merge_intervals(walls);
    let solid = subtract_openings(merged, openings);

    let mut added = 0usize;
    // Ground line
    doc.add_geometry(
        "A-SECT",
        Geometry::Line(Segment::new(
            Point2::new(0.0, 0.0),
            Point2::new(cut_len, 0.0),
        )),
    );
    added += 1;
    // Ceiling line
    doc.add_geometry(
        "A-SECT",
        Geometry::Line(Segment::new(
            Point2::new(0.0, storey_height),
            Point2::new(cut_len, storey_height),
        )),
    );
    added += 1;

    for (s0, s1) in solid {
        if s1 - s0 < 1e-6 {
            continue;
        }
        // Wall rectangle in section.
        let poly = Polyline::new(
            vec![
                Point2::new(s0, 0.0),
                Point2::new(s1, 0.0),
                Point2::new(s1, storey_height),
                Point2::new(s0, storey_height),
            ],
            true,
        );
        doc.add_geometry("A-SECT-PATT", Geometry::Polyline(poly));
        added += 1;
    }

    // Cut indicator on plan (dashed mark on A-SECT-IDEN at cut ends).
    doc.add_geometry("A-SECT-IDEN", Geometry::Line(Segment::new(cut_a, cut_b)));
    added += 1;

    Ok(added)
}

fn merge_intervals(mut v: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    if v.is_empty() {
        return v;
    }
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut out = vec![v[0]];
    for (a, b) in v.into_iter().skip(1) {
        let last = out.last_mut().unwrap();
        if a <= last.1 + 1e-9 {
            last.1 = last.1.max(b);
        } else {
            out.push((a, b));
        }
    }
    out
}

fn subtract_openings(walls: Vec<(f64, f64)>, openings: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut result = walls;
    for (o0, o1) in openings {
        let mut next = Vec::new();
        for (w0, w1) in result {
            if o1 <= w0 || o0 >= w1 {
                next.push((w0, w1));
            } else {
                if o0 > w0 {
                    next.push((w0, o0));
                }
                if o1 < w1 {
                    next.push((o1, w1));
                }
            }
        }
        result = next;
    }
    result
}

fn point_to_segment_dist(p: Point2, s: Segment) -> f64 {
    s.distance_to_point(p)
}

/// Intersection of two segments, if any (inclusive endpoints within eps).
fn segment_intersection(a: Segment, b: Segment) -> Option<Point2> {
    let d1 = Point2::new(a.b.x - a.a.x, a.b.y - a.a.y);
    let d2 = Point2::new(b.b.x - b.a.x, b.b.y - b.a.y);
    let cross = d1.x * d2.y - d1.y * d2.x;
    if cross.abs() < 1e-12 {
        return None;
    }
    let t = ((b.a.x - a.a.x) * d2.y - (b.a.y - a.a.y) * d2.x) / cross;
    let u = ((b.a.x - a.a.x) * d1.y - (b.a.y - a.a.y) * d1.x) / cross;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(Point2::new(a.a.x + t * d1.x, a.a.y + t * d1.y))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_model::Geometry;

    #[test]
    fn section_emits_wall_rects_for_crossing_walls() {
        let mut doc = Document::new();
        // Horizontal wall crossed by vertical cut.
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 5.0), Point2::new(10.0, 5.0))),
        );
        let n =
            generate_section(&mut doc, Point2::new(5.0, 0.0), Point2::new(5.0, 10.0), 3.0).unwrap();
        assert!(n >= 3);
        assert!(doc.entities().iter().any(|e| e.layer == "A-SECT-PATT"));
    }
}
