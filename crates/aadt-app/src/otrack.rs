//! Object-snap tracking (OTRACK) — acquired points + alignment rays.
//!
//! Headless math used by the egui viewport when F11 / `osnap_track` is on.
//! See [ADR-0018](../../../docs/adr/0018-otrack.md).

use std::time::{Duration, Instant};

use aadt_geometry::Point2;

use crate::osnap::SnapKind;

/// Hover dwell before a live snap is acquired (ADR-0018).
pub const ACQUIRE_DWELL: Duration = Duration::from_millis(250);

/// Maximum acquired tracking points.
pub const MAX_ACQUIRED: usize = 7;

/// One acquired snap used as a tracking origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcquiredPoint {
    pub point: Point2,
    pub kind: SnapKind,
}

/// Infinite alignment path through an acquired point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackRay {
    pub origin: Point2,
    /// Unit direction.
    pub dir: Point2,
}

impl TrackRay {
    pub fn new(origin: Point2, dir: Point2) -> Option<Self> {
        let len = (dir.x * dir.x + dir.y * dir.y).sqrt();
        if len < 1e-12 {
            return None;
        }
        Some(Self {
            origin,
            dir: Point2::new(dir.x / len, dir.y / len),
        })
    }

    pub fn project(&self, p: Point2) -> Point2 {
        let vx = p.x - self.origin.x;
        let vy = p.y - self.origin.y;
        let t = vx * self.dir.x + vy * self.dir.y;
        Point2::new(self.origin.x + self.dir.x * t, self.origin.y + self.dir.y * t)
    }

    pub fn dist(&self, p: Point2) -> f64 {
        let q = self.project(p);
        let dx = p.x - q.x;
        let dy = p.y - q.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Intersection of two infinite rays (as lines), or `None` if parallel.
pub fn ray_intersection(a: &TrackRay, b: &TrackRay) -> Option<Point2> {
    let denom = a.dir.x * b.dir.y - a.dir.y * b.dir.x;
    if denom.abs() < 1e-9 {
        return None;
    }
    let dx = b.origin.x - a.origin.x;
    let dy = b.origin.y - a.origin.y;
    let t = (dx * b.dir.y - dy * b.dir.x) / denom;
    Some(Point2::new(
        a.origin.x + a.dir.x * t,
        a.origin.y + a.dir.y * t,
    ))
}

/// Build H/V rays (and optional 45° polar rays) for acquired points.
pub fn tracking_rays(acquired: &[AcquiredPoint], polar: bool) -> Vec<TrackRay> {
    let mut out = Vec::with_capacity(acquired.len() * if polar { 4 } else { 2 });
    for a in acquired {
        if let Some(r) = TrackRay::new(a.point, Point2::new(1.0, 0.0)) {
            out.push(r);
        }
        if let Some(r) = TrackRay::new(a.point, Point2::new(0.0, 1.0)) {
            out.push(r);
        }
        if polar {
            // 45° and 135° cover the diagonal family; opposites are the same line.
            let s = std::f64::consts::FRAC_1_SQRT_2;
            if let Some(r) = TrackRay::new(a.point, Point2::new(s, s)) {
                out.push(r);
            }
            if let Some(r) = TrackRay::new(a.point, Point2::new(s, -s)) {
                out.push(r);
            }
        }
    }
    out
}

/// Nearest tracking candidate to `raw` within `tol` (ray∩ray preferred, else
/// projection onto a ray).
pub fn resolve_tracking(
    acquired: &[AcquiredPoint],
    polar: bool,
    raw: Point2,
    tol: f64,
) -> Option<Point2> {
    if acquired.is_empty() {
        return None;
    }
    let rays = tracking_rays(acquired, polar);
    let mut best_int: Option<(f64, Point2)> = None;
    let mut best_proj: Option<(f64, Point2)> = None;
    let push = |best: &mut Option<(f64, Point2)>, cand: Point2| {
        let dx = cand.x - raw.x;
        let dy = cand.y - raw.y;
        let d = (dx * dx + dy * dy).sqrt();
        if d <= tol && best.as_ref().map_or(true, |(bd, _)| d < *bd) {
            *best = Some((d, cand));
        }
    };

    for i in 0..rays.len() {
        for j in (i + 1)..rays.len() {
            if let Some(p) = ray_intersection(&rays[i], &rays[j]) {
                let on_acq = acquired.iter().any(|a| {
                    let dx = a.point.x - p.x;
                    let dy = a.point.y - p.y;
                    dx * dx + dy * dy < 1e-16
                });
                if !on_acq {
                    push(&mut best_int, p);
                }
            }
        }
        if rays[i].dist(raw) <= tol {
            push(&mut best_proj, rays[i].project(raw));
        }
    }
    best_int
        .map(|(_, p)| p)
        .or_else(|| best_proj.map(|(_, p)| p))
}

/// Mutable OTRACK session: acquired list + hover dwell.
#[derive(Debug, Default)]
pub struct OtrackState {
    pub acquired: Vec<AcquiredPoint>,
    pending: Option<(Point2, Instant)>,
}

impl OtrackState {
    pub fn clear(&mut self) {
        self.acquired.clear();
        self.pending = None;
    }

    /// Feed the current running snap (if any). Acquires after dwell.
    pub fn tick_hover(
        &mut self,
        hit: Option<(SnapKind, Point2)>,
        now: Instant,
        same_tol: f64,
    ) -> bool {
        let Some((kind, p)) = hit else {
            self.pending = None;
            return false;
        };
        if self.acquired.iter().any(|a| {
            let dx = a.point.x - p.x;
            let dy = a.point.y - p.y;
            (dx * dx + dy * dy).sqrt() <= same_tol
        }) {
            self.pending = None;
            return false;
        }
        match self.pending {
            Some((prev, since)) if {
                let dx = prev.x - p.x;
                let dy = prev.y - p.y;
                (dx * dx + dy * dy).sqrt() <= same_tol
            } => {
                if now.duration_since(since) >= ACQUIRE_DWELL {
                    self.try_acquire(AcquiredPoint { point: p, kind }, same_tol);
                    self.pending = None;
                    return true;
                }
            }
            _ => {
                self.pending = Some((p, now));
            }
        }
        false
    }

    /// Immediate acquire (e.g. future Shift+force). Returns true if added.
    pub fn try_acquire(&mut self, pt: AcquiredPoint, same_tol: f64) -> bool {
        if self.acquired.len() >= MAX_ACQUIRED {
            return false;
        }
        if self.acquired.iter().any(|a| {
            let dx = a.point.x - pt.point.x;
            let dy = a.point.y - pt.point.y;
            (dx * dx + dy * dy).sqrt() <= same_tol
        }) {
            return false;
        }
        self.acquired.push(pt);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osnap::SnapKind;

    #[test]
    fn hv_intersection_from_two_acquired() {
        let acquired = [
            AcquiredPoint {
                point: Point2::new(0.0, 0.0),
                kind: SnapKind::Endpoint,
            },
            AcquiredPoint {
                point: Point2::new(10.0, 5.0),
                kind: SnapKind::Endpoint,
            },
        ];
        // Cursor near (10, 0) — H from origin ∩ V from (10,5).
        let hit = resolve_tracking(&acquired, false, Point2::new(10.1, 0.1), 1.0);
        let p = hit.expect("tracking int");
        assert!((p.x - 10.0).abs() < 1e-9);
        assert!((p.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn project_onto_horizontal_ray() {
        let acquired = [AcquiredPoint {
            point: Point2::new(2.0, 3.0),
            kind: SnapKind::Midpoint,
        }];
        let hit = resolve_tracking(&acquired, false, Point2::new(8.0, 3.2), 0.5);
        let p = hit.expect("projection");
        assert!((p.x - 8.0).abs() < 1e-9);
        assert!((p.y - 3.0).abs() < 1e-9);
    }

    #[test]
    fn dwell_acquires_after_threshold() {
        let mut st = OtrackState::default();
        let t0 = Instant::now();
        let hit = Some((SnapKind::Endpoint, Point2::new(1.0, 2.0)));
        assert!(!st.tick_hover(hit, t0, 0.01));
        assert!(st.acquired.is_empty());
        assert!(!st.tick_hover(hit, t0 + Duration::from_millis(100), 0.01));
        assert!(st.tick_hover(hit, t0 + Duration::from_millis(300), 0.01));
        assert_eq!(st.acquired.len(), 1);
        assert_eq!(st.acquired[0].point, Point2::new(1.0, 2.0));
    }

    #[test]
    fn polar_adds_diagonal_intersection() {
        let acquired = [
            AcquiredPoint {
                point: Point2::new(0.0, 0.0),
                kind: SnapKind::Endpoint,
            },
            AcquiredPoint {
                point: Point2::new(4.0, 0.0),
                kind: SnapKind::Endpoint,
            },
        ];
        // 45° from origin ∩ vertical from (4,0) → (4,4)
        let hit = resolve_tracking(&acquired, true, Point2::new(4.05, 4.05), 0.5);
        let p = hit.expect("polar int");
        assert!((p.x - 4.0).abs() < 1e-6);
        assert!((p.y - 4.0).abs() < 1e-6);
    }
}
