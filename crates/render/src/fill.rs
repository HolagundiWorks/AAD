//! Polygon filling: even-odd regions to triangles (slab decomposition), wide polylines.

use aad_doc::LwPolyline;
use aad_geom::{Polyline, Vec2};

/// Triangulate an even-odd region bounded by closed loops (holes allowed) by cutting it into
/// horizontal slabs at every vertex y and emitting trapezoids. Robust for any input; output
/// size is O(slabs × crossings), capped for hostile inputs.
pub fn triangulate_evenodd(loops: &[Vec<Vec2>]) -> Vec<Vec2> {
    let mut edges: Vec<(Vec2, Vec2)> = Vec::new();
    for l in loops {
        let n = l.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let (Some(&a), Some(&b)) = (l.get(i), l.get((i + 1) % n)) else { continue };
            if (a.y - b.y).abs() > 1e-15 && a.is_finite() && b.is_finite() {
                edges.push(if a.y < b.y { (a, b) } else { (b, a) });
            }
        }
    }
    if edges.is_empty() || edges.len() > 200_000 {
        return Vec::new();
    }
    let mut ys: Vec<f64> = edges.iter().flat_map(|(a, b)| [a.y, b.y]).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    edges.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));
    let mut out = Vec::new();
    let mut active: Vec<(Vec2, Vec2)> = Vec::new();
    let mut next = 0usize;
    for w in ys.windows(2) {
        let (Some(&y0), Some(&y1)) = (w.first(), w.get(1)) else { continue };
        let ym = (y0 + y1) / 2.0;
        while let Some(e) = edges.get(next) {
            if e.0.y <= ym {
                active.push(*e);
                next += 1;
            } else {
                break;
            }
        }
        active.retain(|e| e.1.y > ym);
        let xat = |e: &(Vec2, Vec2), y: f64| {
            let t = (y - e.0.y) / (e.1.y - e.0.y);
            e.0.x + (e.1.x - e.0.x) * t
        };
        let mut xs: Vec<(f64, f64, f64)> = active.iter().map(|e| (xat(e, ym), xat(e, y0), xat(e, y1))).collect();
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in xs.chunks(2) {
            if let [l, r] = pair {
                let (a, b, c, d) = (Vec2::new(l.1, y0), Vec2::new(r.1, y0), Vec2::new(r.2, y1), Vec2::new(l.2, y1));
                out.extend_from_slice(&[a, b, c, a, c, d]);
            }
        }
        if out.len() > 6_000_000 {
            break;
        }
    }
    out
}

/// Signed area of a loop (positive counter-clockwise).
fn signed_area(l: &[Vec2]) -> f64 {
    let n = l.len();
    (0..n).filter_map(|i| Some((*l.get(i)?, *l.get((i + 1) % n)?))).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>() / 2.0
}

fn contains(l: &[Vec2], p: Vec2) -> bool {
    let n = l.len();
    let mut inside = false;
    for i in 0..n {
        let (Some(&a), Some(&b)) = (l.get(i), l.get((i + 1) % n)) else { continue };
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// Triangulate the even-odd region of a few closed loops (a glyph, a dimension arrowhead outline)
/// by ear clipping: a few dozen triangles where the slab decomposition of [`triangulate_evenodd`]
/// makes hundreds. Loops nested an even number of levels deep are outlines and the odd ones are
/// holes in the loop around them. The result is checked: its area must equal the area the loops
/// enclose, and when it does not (loops that cross, degenerate input) or the input is large, the
/// slab method answers instead, so the output is always a correct fill.
pub fn triangulate_outline(loops: &[Vec<Vec2>]) -> Vec<Vec2> {
    const MAX_LOOPS: usize = 24;
    const MAX_POINTS: usize = 4000;
    let clean: Vec<Vec<Vec2>> = loops
        .iter()
        .map(|l| {
            let mut v: Vec<Vec2> = Vec::with_capacity(l.len());
            for p in l {
                if v.last().is_none_or(|q| (*q - *p).len() > 1e-12) {
                    v.push(*p);
                }
            }
            if v.len() > 1 && v.first().zip(v.last()).is_some_and(|(a, b)| (*a - *b).len() <= 1e-12) {
                v.pop();
            }
            v
        })
        .filter(|l| l.len() >= 3)
        .collect();
    if clean.is_empty()
        || clean.len() > MAX_LOOPS
        || clean.iter().map(Vec::len).sum::<usize>() > MAX_POINTS
        || clean.iter().flatten().any(|p| !p.is_finite())
    {
        return triangulate_evenodd(loops);
    }
    // Nesting depth of each loop: how many other loops contain its first point.
    let depth: Vec<usize> = clean
        .iter()
        .enumerate()
        .map(|(i, l)| l.first().map_or(0, |p| clean.iter().enumerate().filter(|(j, o)| *j != i && contains(o, *p)).count()))
        .collect();
    let mut out: Vec<Vec2> = Vec::new();
    let mut expected = 0.0;
    for (i, outer) in clean.iter().enumerate().filter(|(i, _)| depth.get(*i).is_some_and(|d| d % 2 == 0)) {
        let holes: Vec<&Vec<Vec2>> = clean
            .iter()
            .enumerate()
            .filter(|(j, h)| depth.get(*j).is_some_and(|d| *d == depth[i] + 1) && h.first().is_some_and(|p| contains(outer, *p)))
            .map(|(_, h)| h)
            .collect();
        let mut data: Vec<f64> = Vec::new();
        let mut hole_at: Vec<usize> = Vec::new();
        for p in outer {
            data.extend_from_slice(&[p.x, p.y]);
        }
        for h in &holes {
            hole_at.push(data.len() / 2);
            for p in *h {
                data.extend_from_slice(&[p.x, p.y]);
            }
        }
        let Ok(idx) = earcutr::earcut(&data, &hole_at, 2) else { return triangulate_evenodd(loops) };
        for i in idx {
            let (Some(x), Some(y)) = (data.get(2 * i), data.get(2 * i + 1)) else { return triangulate_evenodd(loops) };
            out.push(Vec2::new(*x, *y));
        }
        expected += signed_area(outer).abs() - holes.iter().map(|h| signed_area(h).abs()).sum::<f64>();
    }
    let got: f64 = out.as_chunks::<3>().0.iter().map(|t| (t[1] - t[0]).cross(t[2] - t[0]).abs() / 2.0).sum();
    if out.is_empty() || (got - expected).abs() > 1e-6 * expected.abs().max(got.abs()).max(1e-300) {
        return triangulate_evenodd(loops);
    }
    out
}

/// Triangles for a polyline with widths (constant or tapered per segment).
pub fn wide_polyline(p: &LwPolyline, tol: f64) -> Vec<Vec2> {
    let pl = Polyline { vertices: p.vertices.clone(), closed: p.closed };
    let mut out = Vec::new();
    let n = p.vertices.len();
    for (i, seg) in pl.segments().iter().enumerate() {
        let v = p.vertices.get(i % n.max(1)).copied().unwrap_or_default();
        let (w0, w1) = if p.const_width > 0.0 { (p.const_width, p.const_width) } else { (v.start_width, v.end_width) };
        let mut pts = Vec::new();
        seg.tessellate(tol, &mut pts);
        let m = pts.len();
        if m < 2 {
            continue;
        }
        let mut prev: Option<(Vec2, Vec2)> = None;
        for (k, q) in pts.iter().enumerate() {
            let t = k as f64 / (m - 1) as f64;
            let w = (w0 + (w1 - w0) * t) / 2.0;
            let dir = if k + 1 < m { pts.get(k + 1).map(|n| *n - *q) } else { pts.get(k.wrapping_sub(1)).map(|pv| *q - *pv) }
                .unwrap_or(Vec2::X)
                .normalized();
            let nrm = dir.perp() * w;
            let cur = (*q + nrm, *q - nrm);
            if let Some((a, b)) = prev {
                out.extend_from_slice(&[a, b, cur.1, a, cur.1, cur.0]);
            }
            prev = Some(cur);
        }
    }
    out
}

#[cfg(test)]
mod tests {

    fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                Vec2::new(cx + r * (i as f64 * std::f64::consts::TAU / n as f64).cos(), cy + r * (i as f64 * std::f64::consts::TAU / n as f64).sin())
            })
            .collect()
    }

    #[test]
    fn an_outline_with_holes_is_filled_exactly_and_with_far_fewer_triangles_than_slabs() {
        // An "8": an outer ring with two separate holes, one of them wound the other way.
        let outer = circle(0.0, 0.0, 10.0, 64);
        let mut h1 = circle(0.0, 4.0, 3.0, 32);
        let h2 = circle(0.0, -4.0, 3.0, 32);
        h1.reverse();
        let loops = vec![outer, h1, h2];
        let ear = triangulate_outline(&loops);
        let slab = triangulate_evenodd(&loops);
        let expected = |t: &[Vec2]| area(t);
        assert!((expected(&ear) - expected(&slab)).abs() < 1e-6 * expected(&slab), "same area: {} vs {}", expected(&ear), expected(&slab));
        assert!(ear.len() < slab.len(), "ear clipping {} vs slabs {} vertices", ear.len(), slab.len());
        // A letter with an island inside a hole (like a boxed O): even depth is filled again.
        let loops = vec![circle(0.0, 0.0, 10.0, 48), circle(0.0, 0.0, 6.0, 48), circle(0.0, 0.0, 3.0, 24)];
        let t = triangulate_outline(&loops);
        let want = std::f64::consts::PI * (100.0 - 36.0 + 9.0);
        assert!((area(&t) - want).abs() / want < 0.02, "{} vs {want}", area(&t));
    }

    #[test]
    fn loops_that_cross_or_are_hostile_still_give_a_correct_fill_or_nothing_and_never_panic() {
        // Two overlapping squares: the even-odd result is the two non-shared parts.
        let a = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        let b = vec![Vec2::new(2.0, 2.0), Vec2::new(6.0, 2.0), Vec2::new(6.0, 6.0), Vec2::new(2.0, 6.0)];
        let t = triangulate_outline(&[a.clone(), b.clone()]);
        assert!((area(&t) - 24.0).abs() < 1e-6, "16 + 16 - 2 x 4: {}", area(&t));
        assert!(triangulate_outline(&[]).is_empty());
        assert!(triangulate_outline(&[vec![Vec2::ZERO, Vec2::X]]).is_empty());
        let _ = triangulate_outline(&[vec![Vec2::new(f64::NAN, 0.0), Vec2::X, Vec2::Y]]);
        let _ = triangulate_outline(&[vec![Vec2::ZERO; 5]]);
        let many: Vec<Vec<Vec2>> = (0..100).map(|i| circle(f64::from(i), 0.0, 0.4, 8)).collect();
        let _ = triangulate_outline(&many);
        let huge = vec![circle(0.0, 0.0, 1e12, 5000)];
        let _ = triangulate_outline(&huge);
    }
    use super::*;

    fn area(tris: &[Vec2]) -> f64 {
        tris.chunks(3).map(|t| ((t[1] - t[0]).cross(t[2] - t[0]) / 2.0).abs()).sum()
    }

    #[test]
    fn square_with_hole() {
        let outer = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)];
        let hole = vec![Vec2::new(4.0, 4.0), Vec2::new(6.0, 4.0), Vec2::new(6.0, 6.0), Vec2::new(4.0, 6.0)];
        let t = triangulate_evenodd(&[outer, hole]);
        assert!((area(&t) - 96.0).abs() < 1e-9);
    }

    #[test]
    fn triangle_area() {
        let t = triangulate_evenodd(&[vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(0.0, 3.0)]]);
        assert!((area(&t) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn degenerate_input() {
        assert!(triangulate_evenodd(&[vec![Vec2::ZERO, Vec2::X]]).is_empty());
        // NaN input must not panic; the result itself is unspecified.
        let _ = triangulate_evenodd(&[vec![Vec2::new(f64::NAN, 0.0), Vec2::X, Vec2::Y]]);
    }
}
