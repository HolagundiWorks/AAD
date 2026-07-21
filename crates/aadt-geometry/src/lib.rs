//! # aadt-geometry — 2D geometry kernel (substrate, spec §3 / §4)
//!
//! The deterministic math the whole product stands on: points, vectors, segments,
//! polylines, tolerances, bounding boxes, and the parallel/offset/overlap tests the
//! wall detector needs. 2D-only by design (spec §3.1) — no B-rep, no ACIS/Parasolid.
//!
//! M0 ships the subset consumed by `aadt-detect` and `aadt-takeoff`. Arcs, splines,
//! and a spatial index arrive with **M1 — CAD Core Substrate** (see
//! `docs/ROADMAP.md`).

/// Absolute tolerance (drawing units) for treating two coordinates as coincident.
pub const LENGTH_EPS: f64 = 1e-6;

/// Angular tolerance (radians) for treating two directions as parallel.
pub const ANGLE_EPS: f64 = 1e-4;

/// A 2D point in drawing coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Euclidean distance to another point.
    pub fn distance(self, other: Point2) -> f64 {
        (self - other).length()
    }

    /// True when two points are within [`LENGTH_EPS`].
    pub fn coincident(self, other: Point2) -> bool {
        self.distance(other) <= LENGTH_EPS
    }

    /// Midpoint between two points.
    pub fn midpoint(self, other: Point2) -> Point2 {
        Point2::new((self.x + other.x) * 0.5, (self.y + other.y) * 0.5)
    }

    /// This point rotated `radians` (CCW) about `pivot`.
    pub fn rotate_about(self, pivot: Point2, radians: f64) -> Point2 {
        let (sin, cos) = radians.sin_cos();
        let dx = self.x - pivot.x;
        let dy = self.y - pivot.y;
        Point2::new(pivot.x + dx * cos - dy * sin, pivot.y + dx * sin + dy * cos)
    }

    /// This point scaled by `factor` about `pivot`. A negative factor reflects
    /// through the pivot.
    pub fn scale_about(self, pivot: Point2, factor: f64) -> Point2 {
        Point2::new(
            pivot.x + (self.x - pivot.x) * factor,
            pivot.y + (self.y - pivot.y) * factor,
        )
    }

    /// This point reflected across the line through `a` and `b`. A degenerate
    /// axis (`a == b`) leaves the point unchanged.
    pub fn mirror_across(self, a: Point2, b: Point2) -> Point2 {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let denom = dx * dx + dy * dy;
        if denom <= LENGTH_EPS * LENGTH_EPS {
            return self;
        }
        // Project onto the axis, then reflect across the projection.
        let t = ((self.x - a.x) * dx + (self.y - a.y) * dy) / denom;
        let px = a.x + t * dx;
        let py = a.y + t * dy;
        Point2::new(2.0 * px - self.x, 2.0 * py - self.y)
    }
}

impl std::ops::Sub for Point2 {
    type Output = Vec2;
    fn sub(self, rhs: Point2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Add<Vec2> for Point2 {
    type Output = Point2;
    fn add(self, rhs: Vec2) -> Point2 {
        Point2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

/// A 2D vector / displacement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn dot(self, other: Vec2) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// 2D cross product (z-component of the 3D cross).
    pub fn cross(self, other: Vec2) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// Unit vector, or `None` for a (near-)zero-length vector.
    pub fn normalized(self) -> Option<Vec2> {
        let len = self.length();
        if len <= LENGTH_EPS {
            None
        } else {
            Some(Vec2::new(self.x / len, self.y / len))
        }
    }

    /// The left normal — this vector rotated 90° CCW. For a unit direction it is
    /// the unit normal pointing to the left of travel, the `+` side an OFFSET
    /// moves toward by default.
    pub fn perp(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }
}

impl std::ops::Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, rhs: f64) -> Vec2 {
        Vec2::new(self.x * rhs, self.y * rhs)
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

/// A straight segment between two endpoints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub a: Point2,
    pub b: Point2,
}

impl Segment {
    pub const fn new(a: Point2, b: Point2) -> Self {
        Self { a, b }
    }

    pub fn length(self) -> f64 {
        self.a.distance(self.b)
    }

    pub fn midpoint(self) -> Point2 {
        self.a.midpoint(self.b)
    }

    /// Unit direction from `a` to `b`, or `None` for a degenerate segment.
    pub fn direction(self) -> Option<Vec2> {
        (self.b - self.a).normalized()
    }

    /// True when the two segments' directions are parallel (either orientation)
    /// within [`ANGLE_EPS`]. Degenerate segments are never parallel.
    pub fn is_parallel(self, other: Segment) -> bool {
        match (self.direction(), other.direction()) {
            (Some(d1), Some(d2)) => d1.cross(d2).abs() <= ANGLE_EPS,
            _ => false,
        }
    }

    /// Perpendicular distance from a point to the infinite line through this
    /// segment. For a degenerate segment, falls back to the distance to `a`.
    pub fn perpendicular_distance(self, p: Point2) -> f64 {
        match self.direction() {
            Some(d) => (p - self.a).cross(d).abs(),
            None => p.distance(self.a),
        }
    }

    /// The point at parameter `t` (drawing units) measured from `a` along the
    /// segment's direction. Returns `a` for a degenerate segment.
    pub fn point_at(self, t: f64) -> Point2 {
        match self.direction() {
            Some(d) => self.a + d * t,
            None => self.a,
        }
    }

    /// Shortest distance from `p` to the segment itself, clamped to the
    /// endpoints.
    ///
    /// Differs from [`perpendicular_distance`](Self::perpendicular_distance),
    /// which measures to the *infinite* line: a point far beyond a segment's end
    /// can sit on that line at distance zero while being nowhere near the
    /// segment.
    pub fn distance_to_point(self, p: Point2) -> f64 {
        match self.direction() {
            Some(d) => {
                let t = (p - self.a).dot(d).clamp(0.0, self.length());
                (self.a + d * t).distance(p)
            }
            None => p.distance(self.a),
        }
    }

    /// Perpendicular foot of `p` on the infinite line through this segment.
    pub fn foot_of(self, p: Point2) -> Point2 {
        match self.direction() {
            Some(d) => self.a + d * (p - self.a).dot(d),
            None => self.a,
        }
    }

    /// The `[lo, hi]` parameter interval (from `a`, along this segment's direction)
    /// over which a parallel `other` overlaps this segment. `None` when the
    /// segments are not parallel or do not overlap.
    pub fn overlap_interval(self, other: Segment) -> Option<(f64, f64)> {
        let dir = self.direction()?;
        if !self.is_parallel(other) {
            return None;
        }
        let mut o0 = (other.a - self.a).dot(dir);
        let mut o1 = (other.b - self.a).dot(dir);
        if o0 > o1 {
            std::mem::swap(&mut o0, &mut o1);
        }
        let lo = 0.0_f64.max(o0);
        let hi = self.length().min(o1);
        if hi - lo <= LENGTH_EPS {
            None
        } else {
            Some((lo, hi))
        }
    }

    /// Length over which this segment and a parallel `other` overlap when both are
    /// projected onto this segment's direction. Returns `0.0` if they don't overlap
    /// or if directions are not parallel.
    pub fn parallel_overlap(self, other: Segment) -> f64 {
        self.overlap_interval(other)
            .map(|(lo, hi)| hi - lo)
            .unwrap_or(0.0)
    }

    /// The parameter `t` along this segment (`a` at 0, `b` at 1) closest to `p`.
    /// The intersection helpers report positions in this parameterisation, so a
    /// caller can tell whether a hit is inside the segment (`0..=1`) or on its
    /// extension.
    pub fn project_param(self, p: Point2) -> f64 {
        let r = self.b - self.a;
        let len2 = r.dot(r);
        if len2 <= LENGTH_EPS * LENGTH_EPS {
            0.0
        } else {
            (p - self.a).dot(r) / len2
        }
    }

    /// Intersection of the two *infinite* lines through these segments, plus the
    /// parameter of that point on `self` (`a` at 0, `b` at 1). `None` when the
    /// lines are parallel. This is the primitive OFFSET (mitred poly-lines),
    /// EXTEND, TRIM, and FILLET all build on.
    pub fn line_intersection(self, other: Segment) -> Option<(Point2, f64)> {
        let r = self.b - self.a;
        let s = other.b - other.a;
        let denom = r.cross(s);
        if denom.abs() <= LENGTH_EPS {
            return None;
        }
        let t = (other.a - self.a).cross(s) / denom;
        Some((self.a + r * t, t))
    }

    /// Intersection point where the two *bounded* segments actually cross, within
    /// a small tolerance, else `None`. Unlike [`line_intersection`](Self::line_intersection)
    /// this rejects a crossing that lies on either segment's extension.
    pub fn segment_intersection(self, other: Segment) -> Option<Point2> {
        let r = self.b - self.a;
        let s = other.b - other.a;
        let denom = r.cross(s);
        if denom.abs() <= LENGTH_EPS {
            return None;
        }
        let diff = other.a - self.a;
        let t = diff.cross(s) / denom;
        let u = diff.cross(r) / denom;
        let eps = 1e-9;
        let inside = |v: f64| (-eps..=1.0 + eps).contains(&v);
        (inside(t) && inside(u)).then(|| self.a + r * t)
    }
}

/// A circle — used for round columns and other radial features.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub center: Point2,
    pub radius: f64,
}

impl Circle {
    pub const fn new(center: Point2, radius: f64) -> Self {
        Self { center, radius }
    }

    /// Enclosed area (π r²).
    pub fn area(self) -> f64 {
        std::f64::consts::PI * self.radius * self.radius
    }

    /// Circumference (2 π r).
    pub fn circumference(self) -> f64 {
        2.0 * std::f64::consts::PI * self.radius
    }

    /// Where the *infinite* line through `seg` meets this circle: 0 points (miss),
    /// 1 (tangent), or 2 (secant), ordered along the segment's direction. The
    /// arc-aware TRIM/EXTEND build on this.
    pub fn line_intersections(self, seg: Segment) -> Vec<Point2> {
        let d = seg.b - seg.a;
        let a = d.dot(d);
        if a <= LENGTH_EPS * LENGTH_EPS {
            return Vec::new(); // degenerate segment: no direction
        }
        let f = seg.a - self.center;
        let b = 2.0 * f.dot(d);
        let c = f.dot(f) - self.radius * self.radius;
        let disc = b * b - 4.0 * a * c;
        if disc < -LENGTH_EPS {
            return Vec::new();
        }
        let sq = disc.max(0.0).sqrt();
        let t1 = (-b - sq) / (2.0 * a);
        let t2 = (-b + sq) / (2.0 * a);
        if sq <= LENGTH_EPS {
            vec![seg.a + d * t1] // tangent
        } else {
            vec![seg.a + d * t1, seg.a + d * t2]
        }
    }
}

/// A circular arc — door swings, curved walls, and radial annotation.
///
/// Angles are in **degrees**, counter-clockwise from +x, following the DXF
/// convention so import/export need no unit conversion. The arc runs
/// counter-clockwise from `start_angle` to `end_angle`; when `end_angle` is not
/// greater than `start_angle` it is taken to wrap through 360°, so a
/// 350°→10° arc sweeps 20° rather than -340°.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arc {
    pub center: Point2,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

impl Arc {
    pub const fn new(center: Point2, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Self {
            center,
            radius,
            start_angle,
            end_angle,
        }
    }

    /// Swept angle in degrees, always in `(0, 360]` for a non-degenerate arc.
    pub fn sweep(self) -> f64 {
        let mut d = self.end_angle - self.start_angle;
        while d <= 0.0 {
            d += 360.0;
        }
        while d > 360.0 {
            d -= 360.0;
        }
        d
    }

    /// Arc length along the curve.
    pub fn length(self) -> f64 {
        self.radius * self.sweep().to_radians()
    }

    /// The point at `t` along the sweep, `t` in `[0, 1]`.
    pub fn point_at(self, t: f64) -> Point2 {
        let a = (self.start_angle + self.sweep() * t).to_radians();
        Point2::new(
            self.center.x + self.radius * a.cos(),
            self.center.y + self.radius * a.sin(),
        )
    }

    pub fn start_point(self) -> Point2 {
        self.point_at(0.0)
    }

    pub fn end_point(self) -> Point2 {
        self.point_at(1.0)
    }

    /// The full circle this arc lies on.
    pub fn circle(self) -> Circle {
        Circle::new(self.center, self.radius)
    }

    /// The angle (degrees, `[0, 360)`) from the centre to `p`, whether or not `p`
    /// is on the arc.
    pub fn angle_of(self, p: Point2) -> f64 {
        (p.y - self.center.y)
            .atan2(p.x - self.center.x)
            .to_degrees()
            .rem_euclid(360.0)
    }

    /// Whether `deg` falls within the arc's CCW sweep from `start_angle` to
    /// `end_angle` (within a small tolerance).
    pub fn contains_angle(self, deg: f64) -> bool {
        let off = (deg - self.start_angle).rem_euclid(360.0);
        off <= self.sweep() + 1e-6
    }

    /// Whether `p` lies on the arc: on its circle *and* within its sweep.
    pub fn contains_point(self, p: Point2) -> bool {
        (p.distance(self.center) - self.radius).abs() <= 1e-6
            && self.contains_angle(self.angle_of(p))
    }

    /// Approximate the arc as `segments` chords. Used by consumers that only
    /// understand straight edges (rendering, length take-off); `segments` is
    /// clamped to at least 1.
    pub fn to_polyline(self, segments: usize) -> Polyline {
        let n = segments.max(1);
        let verts = (0..=n)
            .map(|i| self.point_at(i as f64 / n as f64))
            .collect();
        Polyline::new(verts, false)
    }
}

/// An axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bbox {
    pub min: Point2,
    pub max: Point2,
}

impl Bbox {
    /// Bounding box of a set of points, or `None` if the iterator is empty.
    pub fn of_points(points: impl IntoIterator<Item = Point2>) -> Option<Bbox> {
        let mut it = points.into_iter();
        let first = it.next()?;
        let mut bbox = Bbox {
            min: first,
            max: first,
        };
        for p in it {
            bbox.min.x = bbox.min.x.min(p.x);
            bbox.min.y = bbox.min.y.min(p.y);
            bbox.max.x = bbox.max.x.max(p.x);
            bbox.max.y = bbox.max.y.max(p.y);
        }
        Some(bbox)
    }

    pub fn width(self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(self) -> f64 {
        self.max.y - self.min.y
    }
}

/// An open or closed polyline (sequence of vertices).
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    pub vertices: Vec<Point2>,
    pub closed: bool,
}

impl Polyline {
    pub fn new(vertices: Vec<Point2>, closed: bool) -> Self {
        Self { vertices, closed }
    }

    /// The segments of the polyline, including the closing segment when `closed`.
    pub fn segments(&self) -> Vec<Segment> {
        let n = self.vertices.len();
        if n < 2 {
            return Vec::new();
        }
        let mut segs = Vec::with_capacity(if self.closed { n } else { n - 1 });
        for i in 0..n - 1 {
            segs.push(Segment::new(self.vertices[i], self.vertices[i + 1]));
        }
        if self.closed {
            segs.push(Segment::new(self.vertices[n - 1], self.vertices[0]));
        }
        segs
    }

    /// Total length along the polyline (includes the closing segment when closed).
    pub fn length(&self) -> f64 {
        self.segments().iter().map(|s| s.length()).sum()
    }

    /// Signed area via the shoelace formula. Positive for counter-clockwise
    /// vertex order. Meaningful only for closed rings; `0.0` for < 3 vertices.
    pub fn signed_area(&self) -> f64 {
        let v = &self.vertices;
        let n = v.len();
        if n < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for i in 0..n {
            let j = (i + 1) % n;
            sum += v[i].x * v[j].y - v[j].x * v[i].y;
        }
        sum * 0.5
    }

    /// Unsigned enclosed area (absolute value of [`signed_area`](Self::signed_area)).
    pub fn area(&self) -> f64 {
        self.signed_area().abs()
    }

    /// Perimeter of the ring (sum of all segment lengths, closing segment included).
    pub fn perimeter(&self) -> f64 {
        self.length()
    }

    /// Area-weighted centroid of the closed ring, or the vertex average as a
    /// fallback for degenerate (near-zero-area) rings.
    pub fn centroid(&self) -> Option<Point2> {
        let v = &self.vertices;
        let n = v.len();
        if n == 0 {
            return None;
        }
        let a = self.signed_area();
        if a.abs() <= LENGTH_EPS {
            let (sx, sy) = v
                .iter()
                .fold((0.0, 0.0), |(sx, sy), p| (sx + p.x, sy + p.y));
            return Some(Point2::new(sx / n as f64, sy / n as f64));
        }
        let mut cx = 0.0;
        let mut cy = 0.0;
        for i in 0..n {
            let j = (i + 1) % n;
            let cross = v[i].x * v[j].y - v[j].x * v[i].y;
            cx += (v[i].x + v[j].x) * cross;
            cy += (v[i].y + v[j].y) * cross;
        }
        let factor = 1.0 / (6.0 * a);
        Some(Point2::new(cx * factor, cy * factor))
    }

    /// Whether `p` lies inside the closed ring, by crossing number (ray casting
    /// along +x). Meaningful only for closed rings; always `false` for < 3
    /// vertices or an open polyline.
    ///
    /// Points exactly on the boundary are not guaranteed either way — the test
    /// is used to decide room containment, where callers compare centroids that
    /// sit well inside or well outside, never on the edge.
    pub fn contains_point(&self, p: Point2) -> bool {
        if !self.closed || self.vertices.len() < 3 {
            return false;
        }
        let v = &self.vertices;
        let n = v.len();
        let mut inside = false;
        for i in 0..n {
            let j = (i + 1) % n;
            let (a, b) = (v[i], v[j]);
            // Does the edge straddle the horizontal ray through p?
            if (a.y > p.y) != (b.y > p.y) {
                // x of the edge at height p.y.
                let t = (p.y - a.y) / (b.y - a.y);
                if p.x < a.x + t * (b.x - a.x) {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_coincidence() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(3.0, 4.0);
        assert!((a.distance(b) - 5.0).abs() < 1e-9);
        assert!(a.coincident(Point2::new(0.0, 5e-7)));
        assert!(!a.coincident(b));
    }

    #[test]
    fn rotate_scale_mirror_about_a_pivot() {
        let p = Point2::new(2.0, 0.0);
        let o = Point2::new(0.0, 0.0);
        // 90° CCW about the origin takes (2,0) to (0,2).
        let r = p.rotate_about(o, std::f64::consts::FRAC_PI_2);
        assert!(r.distance(Point2::new(0.0, 2.0)) < 1e-9);
        // A full turn is the identity.
        let full = p.rotate_about(o, std::f64::consts::TAU);
        assert!(full.distance(p) < 1e-9);
        // Scale ×3 about the origin.
        assert!(p.scale_about(o, 3.0).distance(Point2::new(6.0, 0.0)) < 1e-9);
        // Rotating about the pivot itself is a no-op.
        assert!(o.rotate_about(o, 1.234).distance(o) < 1e-9);
    }

    #[test]
    fn mirror_across_an_axis_reflects() {
        let x_axis = (Point2::new(0.0, 0.0), Point2::new(1.0, 0.0));
        // Reflecting (3,2) across the x-axis flips y.
        let m = Point2::new(3.0, 2.0).mirror_across(x_axis.0, x_axis.1);
        assert!(m.distance(Point2::new(3.0, -2.0)) < 1e-9);
        // A point on the axis is its own reflection.
        let on = Point2::new(5.0, 0.0).mirror_across(x_axis.0, x_axis.1);
        assert!(on.distance(Point2::new(5.0, 0.0)) < 1e-9);
        // A degenerate axis leaves the point unchanged.
        let p = Point2::new(1.0, 1.0);
        assert!(p.mirror_across(p, p).distance(p) < 1e-9);
    }

    #[test]
    fn perp_is_the_left_normal() {
        let d = Vec2::new(1.0, 0.0);
        let n = d.perp();
        assert!((n.x).abs() < 1e-9 && (n.y - 1.0).abs() < 1e-9, "+x → +y");
        // Perpendicular to the source, unit-length preserved.
        assert!(d.dot(n).abs() < 1e-9);
    }

    #[test]
    fn line_and_segment_intersection() {
        let horiz = Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0));
        let vert = Segment::new(Point2::new(4.0, -5.0), Point2::new(4.0, 5.0));
        let (p, t) = horiz.line_intersection(vert).unwrap();
        assert!(p.distance(Point2::new(4.0, 0.0)) < 1e-9);
        assert!(
            (t - 0.4).abs() < 1e-9,
            "0.4 of the way along the horizontal"
        );
        assert!(
            horiz.segment_intersection(vert).is_some(),
            "they actually cross"
        );

        // Parallel lines never intersect.
        let par = Segment::new(Point2::new(0.0, 1.0), Point2::new(10.0, 1.0));
        assert!(horiz.line_intersection(par).is_none());

        // Infinite lines meet, but the bounded segments do not.
        let away = Segment::new(Point2::new(20.0, -5.0), Point2::new(20.0, 5.0));
        assert!(horiz.line_intersection(away).is_some());
        assert!(
            horiz.segment_intersection(away).is_none(),
            "hit is past the segment"
        );
    }

    #[test]
    fn circle_line_intersections() {
        let c = Circle::new(Point2::new(0.0, 0.0), 5.0);
        // A horizontal line through the centre cuts at (±5, 0).
        let secant = Segment::new(Point2::new(-10.0, 0.0), Point2::new(10.0, 0.0));
        let hits = c.line_intersections(secant);
        assert_eq!(hits.len(), 2);
        assert!(hits[0].distance(Point2::new(-5.0, 0.0)) < 1e-9);
        assert!(hits[1].distance(Point2::new(5.0, 0.0)) < 1e-9);
        // A tangent line touches once.
        let tangent = Segment::new(Point2::new(-10.0, 5.0), Point2::new(10.0, 5.0));
        assert_eq!(c.line_intersections(tangent).len(), 1);
        // A miss returns nothing.
        assert!(c
            .line_intersections(Segment::new(
                Point2::new(-10.0, 9.0),
                Point2::new(10.0, 9.0)
            ))
            .is_empty());
    }

    #[test]
    fn arc_angle_and_containment() {
        // A quarter arc from 0° to 90°.
        let a = Arc::new(Point2::new(0.0, 0.0), 2.0, 0.0, 90.0);
        assert!((a.angle_of(Point2::new(0.0, 3.0)) - 90.0).abs() < 1e-9);
        assert!(a.contains_angle(45.0));
        assert!(!a.contains_angle(135.0));
        // Point on the arc vs. on the circle but outside the sweep.
        assert!(a.contains_point(Point2::new(2.0, 0.0)));
        assert!(!a.contains_point(Point2::new(-2.0, 0.0)));
        // Wrap-around: 350°→10° contains 0°.
        let wrap = Arc::new(Point2::new(0.0, 0.0), 1.0, 350.0, 10.0);
        assert!(wrap.contains_angle(0.0) && wrap.contains_angle(355.0));
        assert!(!wrap.contains_angle(180.0));
    }

    #[test]
    fn parallel_and_perpendicular_distance() {
        let s1 = Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0));
        let s2 = Segment::new(Point2::new(0.0, 0.5), Point2::new(10.0, 0.5));
        let s3 = Segment::new(Point2::new(0.0, 0.0), Point2::new(0.0, 10.0));
        assert!(s1.is_parallel(s2));
        assert!(!s1.is_parallel(s3));
        // Parallel offset equals the perpendicular distance of a point on s2.
        assert!((s1.perpendicular_distance(s2.a) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn parallel_overlap_length() {
        let s1 = Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0));
        let s2 = Segment::new(Point2::new(4.0, 0.5), Point2::new(20.0, 0.5));
        assert!((s1.parallel_overlap(s2) - 6.0).abs() < 1e-9);
        let s3 = Segment::new(Point2::new(20.0, 0.5), Point2::new(30.0, 0.5));
        assert_eq!(s1.parallel_overlap(s3), 0.0);
    }

    #[test]
    fn closed_polyline_area_perimeter_centroid() {
        // 4 x 3 rectangle.
        let rect = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
                Point2::new(0.0, 3.0),
            ],
            true,
        );
        assert!((rect.area() - 12.0).abs() < 1e-9);
        assert!((rect.perimeter() - 14.0).abs() < 1e-9);
        let c = rect.centroid().unwrap();
        assert!((c.x - 2.0).abs() < 1e-9 && (c.y - 1.5).abs() < 1e-9);
    }

    #[test]
    fn distance_to_point_clamps_at_the_endpoints() {
        let s = Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0));
        // Alongside the segment: same as the perpendicular distance.
        assert!((s.distance_to_point(Point2::new(5.0, 3.0)) - 3.0).abs() < 1e-9);
        // Beyond the end and collinear: on the infinite line, but 5 from the
        // segment. This is the case perpendicular_distance gets wrong.
        let past = Point2::new(15.0, 0.0);
        assert!(s.perpendicular_distance(past).abs() < 1e-9);
        assert!((s.distance_to_point(past) - 5.0).abs() < 1e-9);
        // Beyond the start, off-axis: distance to the start point.
        assert!((s.distance_to_point(Point2::new(-3.0, 4.0)) - 5.0).abs() < 1e-9);
        // Degenerate segment falls back to point distance.
        let dot = Segment::new(Point2::new(1.0, 1.0), Point2::new(1.0, 1.0));
        assert!((dot.distance_to_point(Point2::new(1.0, 4.0)) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn arc_sweep_length_and_endpoints() {
        // Quarter arc of radius 2, from +x to +y.
        let a = Arc::new(Point2::new(0.0, 0.0), 2.0, 0.0, 90.0);
        assert!((a.sweep() - 90.0).abs() < 1e-9);
        assert!((a.length() - std::f64::consts::PI).abs() < 1e-9);
        let s = a.start_point();
        let e = a.end_point();
        assert!((s.x - 2.0).abs() < 1e-9 && s.y.abs() < 1e-9);
        assert!(e.x.abs() < 1e-9 && (e.y - 2.0).abs() < 1e-9);
    }

    #[test]
    fn arc_sweep_wraps_through_zero() {
        // 350 -> 10 is a 20 degree arc, not -340.
        let a = Arc::new(Point2::new(0.0, 0.0), 1.0, 350.0, 10.0);
        assert!((a.sweep() - 20.0).abs() < 1e-9);
        // A full circle stays 360, not collapsing to 0.
        let full = Arc::new(Point2::new(0.0, 0.0), 1.0, 0.0, 360.0);
        assert!((full.sweep() - 360.0).abs() < 1e-9);
    }

    #[test]
    fn arc_to_polyline_tracks_the_curve() {
        let a = Arc::new(Point2::new(0.0, 0.0), 5.0, 0.0, 180.0);
        let p = a.to_polyline(8);
        assert_eq!(p.vertices.len(), 9, "n segments yields n+1 vertices");
        assert!(!p.closed);
        // Every sampled vertex sits on the circle.
        for v in &p.vertices {
            assert!((v.distance(Point2::new(0.0, 0.0)) - 5.0).abs() < 1e-9);
        }
        // Chords underestimate the true arc length but converge from below.
        assert!(p.length() < a.length());
        assert!(a.to_polyline(256).length() > p.length());
        // Degenerate segment counts do not panic.
        assert_eq!(a.to_polyline(0).vertices.len(), 2);
    }

    #[test]
    fn contains_point_inside_outside_and_concave() {
        let rect = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
                Point2::new(0.0, 3.0),
            ],
            true,
        );
        assert!(rect.contains_point(Point2::new(2.0, 1.5)));
        assert!(!rect.contains_point(Point2::new(5.0, 1.5)));
        assert!(!rect.contains_point(Point2::new(-1.0, 1.5)));
        // Collinear with the top edge but beyond it.
        assert!(!rect.contains_point(Point2::new(2.0, 7.0)));

        // L-shape: the notch must read as outside even though it is within the
        // bounding box — a bbox test would get this wrong.
        let ell = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 1.0),
                Point2::new(1.0, 1.0),
                Point2::new(1.0, 4.0),
                Point2::new(0.0, 4.0),
            ],
            true,
        );
        assert!(ell.contains_point(Point2::new(0.5, 2.0)));
        assert!(ell.contains_point(Point2::new(3.0, 0.5)));
        assert!(!ell.contains_point(Point2::new(3.0, 3.0)));

        // Open polylines and degenerate rings are never containing.
        let open = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
            ],
            false,
        );
        assert!(!open.contains_point(Point2::new(3.0, 1.0)));
    }
}
