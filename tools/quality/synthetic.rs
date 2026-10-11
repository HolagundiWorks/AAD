//! A reproducible synthetic drawing for benchmarks, shared by the bench files of several crates
//! with `#[path = "../../../tools/quality/synthetic.rs"] mod synthetic;` (it is source, not a
//! crate, so no crate has to depend on another just for benchmarking).
//!
//! The mix is one fifth each of lines, circles, arcs, bulging polylines and text, scattered
//! uniformly over a square whose side grows with the square root of the count, so density (and so
//! the work per query) stays the same at every size. The generator is deterministic: the same
//! count always gives the same drawing.

#![allow(dead_code)]

use cadcraft_doc::{self as d, Common, Drawing, EntityKind, Space};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

/// Side of the square the drawing of `n` entities fills.
pub fn side(n: usize) -> f64 {
    (n as f64).sqrt() * 10.0
}

/// A drawing of `n` entities in model space.
pub fn drawing(n: usize) -> Drawing {
    let side = side(n);
    let mut dr = Drawing::new_imperial();
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let v3 = |x: f64, y: f64| Vec3::new(x, y, 0.0);
    for i in 0..n {
        let (x, y) = (r.range(0.0, side), r.range(0.0, side));
        let s = r.range(0.5, 8.0);
        let kind = match i % 5 {
            0 => EntityKind::Line(d::Line { a: v3(x, y), b: v3(x + r.range(-s, s), y + r.range(-s, s)) }),
            1 => EntityKind::Circle(d::Circle { center: v3(x, y), radius: s * 0.5 }),
            2 => EntityKind::Arc(d::Arc { center: v3(x, y), radius: s * 0.5, start: r.range(0.0, 3.0), end: r.range(3.2, 6.2) }),
            3 => EntityKind::LwPolyline(d::LwPolyline {
                vertices: (0..6)
                    .map(|k| PolyVertex {
                        p: Vec2::new(x + f64::from(k) * s * 0.3, y + r.range(-s, s) * 0.3),
                        bulge: if k == 2 { 0.4 } else { 0.0 },
                        start_width: 0.0,
                        end_width: 0.0,
                    })
                    .collect(),
                closed: i % 2 == 0,
                const_width: 0.0,
                elevation: 0.0,
                plinegen: false,
            }),
            _ => EntityKind::Text(d::Text {
                insert: v3(x, y),
                align_pt: None,
                height: s * 0.2,
                value: format!("T{i}"),
                rotation: 0.0,
                width_factor: 1.0,
                oblique: 0.0,
                style: "Standard".into(),
                halign: d::HAlign::Left,
                valign: d::VAlign::Baseline,
            }),
        };
        // Adding to model space cannot fail for a drawing made here.
        let _ = dr.add(&Space::Model, Common::default(), kind);
    }
    dr
}
