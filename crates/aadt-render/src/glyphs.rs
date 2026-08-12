//! TrueType glyph outlines for viewport TEXT (L5).
//!
//! Tessellates bundled Source Sans 3 into world-space line segments so line-list
//! backends (FFI Direct2D, optional wgpu) show readable labels instead of
//! crosshair stubs. Missing glyphs degrade to skip — never panic callers.

use std::sync::LazyLock;

use ab_glyph::{Font, FontRef, OutlineCurve, Point as AbPoint};
use aadt_geometry::Point2;

static FONT_DATA: &[u8] = include_bytes!("../assets/fonts/SourceSans3-Regular.ttf");

static FONT: LazyLock<FontRef<'static>> = LazyLock::new(|| {
    FontRef::try_from_slice(FONT_DATA).expect("bundled SourceSans3-Regular.ttf")
});

/// Steps per quadratic / cubic curve when flattening outlines.
const CURVE_STEPS: usize = 6;

/// World-space line segments for `value` at CAD text height, baseline-left at
/// `position`, rotated `rotation_deg` (CCW). Empty input or total glyph failure
/// yields an empty vec.
pub fn text_outline_segments(
    position: Point2,
    height: f64,
    rotation_deg: f64,
    value: &str,
) -> Vec<(Point2, Point2)> {
    if value.trim().is_empty() || !height.is_finite() || height <= 0.0 {
        return Vec::new();
    }
    let font = &*FONT;
    let Some(upem) = font.units_per_em() else {
        return Vec::new();
    };
    let upem = f64::from(upem);
    if upem <= 0.0 {
        return Vec::new();
    }
    let scale = height / upem;
    let (sin, cos) = rotation_deg.to_radians().sin_cos();

    let map = |fx: f32, fy: f32| -> Point2 {
        let x = f64::from(fx) * scale;
        let y = f64::from(fy) * scale;
        Point2::new(
            position.x + x * cos - y * sin,
            position.y + x * sin + y * cos,
        )
    };

    let mut out = Vec::new();
    let mut pen_x = 0.0f32;
    for ch in value.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        let gid = font.glyph_id(ch);
        if let Some(outline) = font.outline(gid) {
            flatten_outline(&outline.curves, pen_x, &map, &mut out);
        }
        // Unscaled advance is in font units (same space as outline curves).
        pen_x += font.h_advance_unscaled(gid);
    }
    out
}

fn flatten_outline(
    curves: &[OutlineCurve],
    origin_x: f32,
    map: &dyn Fn(f32, f32) -> Point2,
    out: &mut Vec<(Point2, Point2)>,
) {
    let push = |a: AbPoint, b: AbPoint, out: &mut Vec<(Point2, Point2)>, map: &dyn Fn(f32, f32) -> Point2| {
        if (a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 {
            return;
        }
        out.push((map(origin_x + a.x, a.y), map(origin_x + b.x, b.y)));
    };

    for curve in curves {
        match curve {
            OutlineCurve::Line(p0, p1) => {
                push(*p0, *p1, out, map);
            }
            OutlineCurve::Quad(p0, p1, p2) => {
                let mut prev = *p0;
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let p = eval_quad(*p0, *p1, *p2, t);
                    push(prev, p, out, map);
                    prev = p;
                }
            }
            OutlineCurve::Cubic(p0, p1, p2, p3) => {
                let mut prev = *p0;
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let p = eval_cubic(*p0, *p1, *p2, *p3, t);
                    push(prev, p, out, map);
                    prev = p;
                }
            }
        }
    }
}

fn eval_quad(p0: AbPoint, p1: AbPoint, p2: AbPoint, t: f32) -> AbPoint {
    let u = 1.0 - t;
    AbPoint {
        x: u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
        y: u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
    }
}

fn eval_cubic(p0: AbPoint, p1: AbPoint, p2: AbPoint, p3: AbPoint, t: f32) -> AbPoint {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    AbPoint {
        x: uu * u * p0.x + 3.0 * uu * t * p1.x + 3.0 * u * tt * p2.x + tt * t * p3.x,
        y: uu * u * p0.y + 3.0 * uu * t * p1.y + 3.0 * u * tt * p2.y + tt * t * p3.y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_value_yields_no_segments() {
        let segs = text_outline_segments(Point2::new(0.0, 0.0), 1.0, 0.0, "");
        assert!(segs.is_empty());
        let segs = text_outline_segments(Point2::new(0.0, 0.0), 1.0, 0.0, "   ");
        assert!(segs.is_empty());
    }

    #[test]
    fn letter_a_has_outline_segments() {
        let segs = text_outline_segments(Point2::new(10.0, 20.0), 1.0, 0.0, "A");
        assert!(
            segs.len() > 4,
            "expected outline segments for A, got {}",
            segs.len()
        );
        // Insertion should influence geometry (not all at origin).
        let near_insert = segs.iter().any(|(a, b)| {
            (a.x - 10.0).abs() < 2.0
                || (b.x - 10.0).abs() < 2.0
                || (a.y - 20.0).abs() < 2.0
                || (b.y - 20.0).abs() < 2.0
        });
        assert!(near_insert);
    }

    #[test]
    fn non_positive_height_yields_empty() {
        assert!(text_outline_segments(Point2::new(0.0, 0.0), 0.0, 0.0, "A").is_empty());
        assert!(text_outline_segments(Point2::new(0.0, 0.0), -1.0, 0.0, "A").is_empty());
    }
}
