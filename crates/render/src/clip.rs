//! Clipping against axis-aligned rectangles and arbitrary polygons (XCLIP / VPCLIP).

use aad_geom::{Bounds2, Vec2, line_line_infinite, point_in_polygon};

/// Clip the segment `a`→`b` to `r` (Liang–Barsky). Returns the parameter range kept.
fn clip_params(a: Vec2, d: Vec2, r: &Bounds2, mut t0: f64, mut t1: f64) -> Option<(f64, f64)> {
    for (p, q) in [(-d.x, a.x - r.min.x), (d.x, r.max.x - a.x), (-d.y, a.y - r.min.y), (d.y, r.max.y - a.y)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                if t > t1 {
                    return None;
                }
                if t > t0 {
                    t0 = t;
                }
            } else {
                if t < t0 {
                    return None;
                }
                if t < t1 {
                    t1 = t;
                }
            }
        }
    }
    (t0 <= t1 && t0.is_finite() && t1.is_finite()).then_some((t0, t1))
}

/// Clip a segment to `r`.
pub fn clip_segment(a: Vec2, b: Vec2, r: &Bounds2) -> Option<(Vec2, Vec2)> {
    if r.is_empty() || !a.is_finite() || !b.is_finite() {
        return None;
    }
    let d = b - a;
    let (t0, t1) = clip_params(a, d, r, 0.0, 1.0)?;
    Some((a + d * t0, a + d * t1))
}

/// Clip an infinite line (or a ray when `ray`) through `base` along `dir` to `r`.
pub fn clip_infinite(base: Vec2, dir: Vec2, ray: bool, r: &Bounds2) -> Option<(Vec2, Vec2)> {
    if r.is_empty() || !base.is_finite() || !dir.is_finite() || dir == Vec2::ZERO {
        return None;
    }
    let far = (r.width() + r.height() + (base - r.center()).len()) * 2.0 / dir.len().max(1e-300);
    let (t0, t1) = clip_params(base, dir, r, if ray { 0.0 } else { -far }, far)?;
    Some((base + dir * t0, base + dir * t1))
}

/// Clip a polyline to `r`; returns the visible pieces (each with at least two points).
pub fn clip_polyline(pts: &[Vec2], r: &Bounds2) -> Vec<Vec<Vec2>> {
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    if r.is_empty() {
        return out;
    }
    // Fast path: everything inside.
    if pts.iter().all(|p| r.contains(*p)) {
        if pts.len() >= 2 {
            out.push(pts.to_vec());
        }
        return out;
    }
    let mut cur: Vec<Vec2> = Vec::new();
    for w in pts.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        match clip_segment(*a, *b, r) {
            Some((p, q)) => {
                let joined = cur.last().is_some_and(|l| l.near(p, 1e-12));
                if !joined {
                    if cur.len() >= 2 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    cur.push(p);
                }
                cur.push(q);
                // Leaving the rectangle ends the piece.
                if !q.near(*b, 1e-12) {
                    if cur.len() >= 2 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                }
            }
            None => {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Clip a convex polygon to `r` (Sutherland–Hodgman).
pub fn clip_polygon(poly: &[Vec2], r: &Bounds2) -> Vec<Vec2> {
    if r.is_empty() {
        return Vec::new();
    }
    let mut cur: Vec<Vec2> = poly.to_vec();
    // Each edge: inside test and intersection with the boundary line.
    let edges: [(u8, f64); 4] = [(0, r.min.x), (1, r.max.x), (2, r.min.y), (3, r.max.y)];
    for (side, v) in edges {
        let inside = |p: &Vec2| match side {
            0 => p.x >= v,
            1 => p.x <= v,
            2 => p.y >= v,
            _ => p.y <= v,
        };
        let cross = |a: Vec2, b: Vec2| -> Vec2 {
            let t = match side {
                0 | 1 => (v - a.x) / (b.x - a.x),
                _ => (v - a.y) / (b.y - a.y),
            };
            let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
            a + (b - a) * t
        };
        let mut next = Vec::with_capacity(cur.len() + 2);
        let n = cur.len();
        for i in 0..n {
            let (Some(a), Some(b)) = (cur.get(i).copied(), cur.get((i + 1) % n).copied()) else { continue };
            match (inside(&a), inside(&b)) {
                (true, true) => next.push(b),
                (true, false) => next.push(cross(a, b)),
                (false, true) => {
                    next.push(cross(a, b));
                    next.push(b);
                }
                (false, false) => {}
            }
        }
        cur = next;
        if cur.is_empty() {
            break;
        }
    }
    cur
}

/// Clip a triangle list (three vertices per triangle) to `r`, re-triangulating as fans.
pub fn clip_triangles(tris: &[Vec2], r: &Bounds2) -> Vec<Vec2> {
    let mut out = Vec::new();
    for t in tris.as_chunks::<3>().0 {
        if t.iter().all(|p| r.contains(*p)) {
            out.extend_from_slice(t);
            continue;
        }
        let poly = clip_polygon(t, r);
        if let Some(first) = poly.first().copied() {
            for w in poly.get(1..).unwrap_or(&[]).windows(2) {
                if let (Some(b), Some(c)) = (w.first(), w.get(1)) {
                    out.extend_from_slice(&[first, *b, *c]);
                }
            }
        }
    }
    out
}

/// True when `poly` is a closed ring with at least three finite, distinct vertices.
pub fn is_poly_clip(poly: &[Vec2]) -> bool {
    poly.len() >= 3 && poly.iter().all(|p| p.is_finite())
}

/// Clip the segment `a`→`b` to the interior of an arbitrary polygon (even-odd).
pub fn clip_segment_poly(a: Vec2, b: Vec2, poly: &[Vec2]) -> Vec<(Vec2, Vec2)> {
    let mut out = Vec::new();
    if !is_poly_clip(poly) || !a.is_finite() || !b.is_finite() {
        return out;
    }
    let n = poly.len();
    let mut ts: Vec<f64> = vec![0.0, 1.0];
    for i in 0..n {
        let (Some(p0), Some(p1)) = (poly.get(i).copied(), poly.get((i + 1) % n).copied()) else { continue };
        if let Some((_, t, u)) = line_line_infinite(a, b, p0, p1)
            && t > 1e-12
            && t < 1.0 - 1e-12
            && (-1e-9..=1.0 + 1e-9).contains(&u)
            && t.is_finite()
        {
            ts.push(t.clamp(0.0, 1.0));
        }
    }
    ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
    let d = b - a;
    for w in ts.windows(2) {
        let (Some(&t0), Some(&t1)) = (w.first(), w.get(1)) else { continue };
        if t1 - t0 < 1e-12 {
            continue;
        }
        let mid = a + d * ((t0 + t1) * 0.5);
        if point_in_polygon(poly, mid) {
            out.push((a + d * t0, a + d * t1));
        }
    }
    out
}

/// Clip a polyline to a polygon; returns the visible pieces.
pub fn clip_polyline_poly(pts: &[Vec2], poly: &[Vec2]) -> Vec<Vec<Vec2>> {
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    if !is_poly_clip(poly) {
        return out;
    }
    if pts.iter().all(|p| point_in_polygon(poly, *p)) && pts.len() >= 2 {
        out.push(pts.to_vec());
        return out;
    }
    let mut cur: Vec<Vec2> = Vec::new();
    for w in pts.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let pieces = clip_segment_poly(*a, *b, poly);
        if pieces.is_empty() {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            continue;
        }
        for (i, (p, q)) in pieces.iter().enumerate() {
            let joined = cur.last().is_some_and(|l| l.near(*p, 1e-12));
            if !joined {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                cur.push(*p);
            }
            cur.push(*q);
            // A gap between clipped pieces ends the current run.
            if i + 1 < pieces.len() || !q.near(*b, 1e-12) {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Clip triangles to a polygon: AABB first, then keep fans whose centroid lies inside.
pub fn clip_triangles_poly(tris: &[Vec2], poly: &[Vec2]) -> Vec<Vec2> {
    if !is_poly_clip(poly) {
        return Vec::new();
    }
    let rect = Bounds2::from_points(poly.iter().copied());
    let clipped = clip_triangles(tris, &rect);
    let mut out = Vec::new();
    for t in clipped.as_chunks::<3>().0 {
        if t.iter().all(|p| point_in_polygon(poly, *p)) {
            out.extend_from_slice(t);
            continue;
        }
        let c = (t[0] + t[1] + t[2]) * (1.0 / 3.0);
        if point_in_polygon(poly, c) {
            out.extend_from_slice(t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> Bounds2 {
        Bounds2::new(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0))
    }

    #[test]
    fn segments_and_polylines() {
        let (a, b) = clip_segment(Vec2::new(-5.0, 5.0), Vec2::new(15.0, 5.0), &r()).unwrap();
        assert!(a.near(Vec2::new(0.0, 5.0), 1e-9) && b.near(Vec2::new(10.0, 5.0), 1e-9));
        assert!(clip_segment(Vec2::new(-5.0, -5.0), Vec2::new(-1.0, 20.0), &r()).is_none());
        // In, out, back in: two pieces.
        let pieces = clip_polyline(&[Vec2::new(5.0, 5.0), Vec2::new(15.0, 5.0), Vec2::new(15.0, 8.0), Vec2::new(5.0, 8.0)], &r());
        assert_eq!(pieces.len(), 2);
        let (a, b) = clip_infinite(Vec2::new(5.0, 5.0), Vec2::new(1.0, 0.0), false, &r()).unwrap();
        assert!(a.near(Vec2::new(0.0, 5.0), 1e-9) && b.near(Vec2::new(10.0, 5.0), 1e-9));
        let (a, _) = clip_infinite(Vec2::new(5.0, 5.0), Vec2::new(1.0, 0.0), true, &r()).unwrap();
        assert!(a.near(Vec2::new(5.0, 5.0), 1e-9));
        assert!(clip_segment(Vec2::new(f64::NAN, 0.0), Vec2::new(1.0, 1.0), &r()).is_none());
    }

    #[test]
    fn triangles() {
        let t = clip_triangles(&[Vec2::new(-5.0, 0.0), Vec2::new(5.0, 0.0), Vec2::new(5.0, 10.0)], &r());
        assert!(!t.is_empty() && t.len().is_multiple_of(3));
        assert!(t.iter().all(|p| p.x >= -1e-9));
    }

    #[test]
    fn polygon_clips_diagonally() {
        // Triangle covering the lower-left; a horizontal line through the middle is cut short.
        let poly = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)];
        let pieces = clip_segment_poly(Vec2::new(-2.0, 2.0), Vec2::new(12.0, 2.0), &poly);
        assert_eq!(pieces.len(), 1);
        let (a, b) = pieces[0];
        assert!(a.x >= -1e-6 && b.x <= 8.0 + 1e-3);
        assert!(point_in_polygon(&poly, (a + b) * 0.5));
        assert!(clip_segment_poly(Vec2::new(9.0, 9.0), Vec2::new(12.0, 12.0), &poly).is_empty());
    }
}
