//! A dependency-free PDF plotter — plot a [`Document`] to a vector PDF.
//!
//! Architects transmit drawings as PDF, so plotting is table stakes. Like the
//! `.xlsx` writer, this hand-writes the format rather than pulling in a PDF
//! crate: a PDF is a set of numbered objects (catalog → pages → page → content
//! stream) followed by a cross-reference table and trailer. The content stream
//! uses PDF's own path operators (`m`/`l`/`S`) and text operators (`BT…Tj…ET`).
//!
//! **Deterministic:** no timestamps are written and objects are emitted in a
//! fixed order, so the same drawing produces the same bytes — the `.pdf` can be
//! golden-tested the way the takeoff CSV is. PDF's coordinate origin is the
//! bottom-left with Y up, which matches AADT world space, so plotting needs only
//! a fit transform, no flip.

use aadt_geometry::{Arc, Bbox, Point2};
use aadt_model::{Document, Geometry};

/// A page size in PDF points (1/72 inch).
#[derive(Debug, Clone, Copy)]
pub struct PageSize {
    pub width: f64,
    pub height: f64,
}

impl PageSize {
    pub const A4_LANDSCAPE: PageSize = PageSize {
        width: 842.0,
        height: 595.0,
    };
    pub const A4_PORTRAIT: PageSize = PageSize {
        width: 595.0,
        height: 842.0,
    };
    pub const A3_LANDSCAPE: PageSize = PageSize {
        width: 1191.0,
        height: 842.0,
    };
    pub const A3_PORTRAIT: PageSize = PageSize {
        width: 842.0,
        height: 1191.0,
    };
    pub const LETTER_LANDSCAPE: PageSize = PageSize {
        width: 792.0,
        height: 612.0,
    };

    /// Convert a sheet extent in millimetres to PDF points (1 pt = 1/72 in).
    pub fn from_mm(width_mm: f64, height_mm: f64) -> Self {
        const MM_TO_PT: f64 = 72.0 / 25.4;
        Self {
            width: width_mm * MM_TO_PT,
            height: height_mm * MM_TO_PT,
        }
    }
}

/// Plot `doc` to a single-page PDF, fitting its extents into the page with a
/// margin. An empty drawing yields a valid, blank page.
pub fn plot(doc: &Document, page: PageSize) -> Vec<u8> {
    let content = build_content(doc, page);
    assemble(&content, page)
}

/// Build the page content stream: a fit transform, then a stroked path per
/// entity, coloured by layer.
fn build_content(doc: &Document, page: PageSize) -> String {
    let mut s = String::new();
    s.push_str("1 w\n"); // 1pt line width.

    let fit = Fit::new(doc, page);
    let mut cur_color = [-1.0; 3];
    for e in doc.entities() {
        let color = layer_color(&e.layer);
        if color != cur_color {
            s.push_str(&format!("{} {} {} RG\n", color[0], color[1], color[2]));
            cur_color = color;
        }
        stroke_entity(&mut s, &e.geometry, &fit);
    }
    s
}

fn stroke_entity(s: &mut String, geom: &Geometry, fit: &Fit) {
    let mv = |s: &mut String, p: Point2| {
        let (x, y) = fit.map(p);
        s.push_str(&format!("{x:.2} {y:.2} m\n"));
    };
    let ln = |s: &mut String, p: Point2| {
        let (x, y) = fit.map(p);
        s.push_str(&format!("{x:.2} {y:.2} l\n"));
    };
    match geom {
        Geometry::Line(seg) => {
            mv(s, seg.a);
            ln(s, seg.b);
            s.push_str("S\n");
        }
        Geometry::Polyline(p) => {
            if let Some((first, rest)) = p.vertices.split_first() {
                mv(s, *first);
                for v in rest {
                    ln(s, *v);
                }
                if p.closed {
                    s.push_str("h\n"); // close path
                }
                s.push_str("S\n");
            }
        }
        Geometry::Circle(c) => {
            stroke_polyline_pts(s, &circle_pts(c.center, c.radius), true, mv, ln)
        }
        Geometry::Arc(a) => stroke_polyline_pts(s, &arc_pts(a), false, mv, ln),
        Geometry::Text(t) => emit_text(s, t, fit),
        Geometry::Insert(i) => tick(s, i.position, fit),
    }
}

fn emit_text(s: &mut String, t: &aadt_model::Text, fit: &Fit) {
    let (x, y) = fit.map(t.position);
    let size = (t.height * fit.scale).clamp(4.0, 72.0);
    let escaped = pdf_escape(&t.value);
    s.push_str("BT\n");
    s.push_str(&format!("/F1 {size:.2} Tf\n"));
    if t.rotation.abs() > 0.01 {
        // PDF user space: rotate about the insertion point (degrees → radians).
        let rad = t.rotation.to_radians();
        let (sn, cs) = (rad.sin(), rad.cos());
        s.push_str(&format!(
            "{cs:.6} {sn:.6} {:.6} {:.6} {x:.2} {y:.2} Tm\n",
            -sn, cs
        ));
    } else {
        s.push_str(&format!("{x:.2} {y:.2} Td\n"));
    }
    s.push_str(&format!("({escaped}) Tj\n"));
    s.push_str("ET\n");
}

fn pdf_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\r' | '\n' => out.push(' '),
            c if c.is_ascii() && !c.is_control() => out.push(c),
            _ => {
                // Helvetica Latin-1-ish: drop unsupported glyphs.
            }
        }
    }
    out
}

fn stroke_polyline_pts(
    s: &mut String,
    pts: &[Point2],
    close: bool,
    mv: impl Fn(&mut String, Point2),
    ln: impl Fn(&mut String, Point2),
) {
    if let Some((first, rest)) = pts.split_first() {
        mv(s, *first);
        for p in rest {
            ln(s, *p);
        }
        if close {
            s.push_str("h\n");
        }
        s.push_str("S\n");
    }
}

fn tick(s: &mut String, at: Point2, fit: &Fit) {
    let (x, y) = fit.map(at);
    let d = 3.0;
    s.push_str(&format!(
        "{:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} m {:.2} {:.2} l S\n",
        x - d,
        y,
        x + d,
        y,
        x,
        y - d,
        x,
        y + d
    ));
}

fn circle_pts(center: Point2, radius: f64) -> Vec<Point2> {
    (0..=48)
        .map(|i| {
            let a = (i as f64 / 48.0) * std::f64::consts::TAU;
            Point2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect()
}

fn arc_pts(a: &Arc) -> Vec<Point2> {
    a.to_polyline(32).vertices
}

/// Fit a document's extents into the page's printable area, preserving aspect.
struct Fit {
    scale: f64,
    offset: (f64, f64),
    origin: Point2,
}

impl Fit {
    fn new(doc: &Document, page: PageSize) -> Self {
        let margin = 36.0; // 0.5 inch
        let (pw, ph) = (page.width - 2.0 * margin, page.height - 2.0 * margin);
        match Bbox::of_points(doc.points()) {
            Some(bbox) => {
                let (bw, bh) = (bbox.width().max(1e-9), bbox.height().max(1e-9));
                let scale = (pw / bw).min(ph / bh);
                // Centre the drawing in the printable area.
                let offset = (
                    margin + (pw - bw * scale) * 0.5,
                    margin + (ph - bh * scale) * 0.5,
                );
                Self {
                    scale,
                    offset,
                    origin: bbox.min,
                }
            }
            None => Self {
                scale: 1.0,
                offset: (margin, margin),
                origin: Point2::new(0.0, 0.0),
            },
        }
    }

    fn map(&self, p: Point2) -> (f64, f64) {
        (
            self.offset.0 + (p.x - self.origin.x) * self.scale,
            self.offset.1 + (p.y - self.origin.y) * self.scale,
        )
    }
}

/// A stroke colour (0..1 RGB) for a layer — dark inks on the white page.
fn layer_color(layer: &str) -> [f64; 3] {
    let u = layer.to_ascii_uppercase();
    if u.starts_with("A-WALL") {
        [0.0, 0.0, 0.0]
    } else if u.starts_with("A-ROOM") {
        [0.1, 0.35, 0.7]
    } else if u.starts_with("A-DOOR") || u.starts_with("A-GLAZ") {
        [0.1, 0.5, 0.2]
    } else if u.starts_with("S-") {
        [0.7, 0.15, 0.1]
    } else {
        [0.3, 0.3, 0.3]
    }
}

/// Assemble the PDF objects, cross-reference table, and trailer around a content
/// stream. Five objects: catalog, pages, page, content, and a base font.
fn assemble(content: &str, page: PageSize) -> Vec<u8> {
    let mut objects: Vec<String> = Vec::new();
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_string());
    objects.push("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string());
    objects.push(format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.0} {:.0}] \
         /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        page.width, page.height
    ));
    objects.push(format!(
        "<< /Length {} >>\nstream\n{content}endstream",
        content.len()
    ));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());

    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }

    let xref_pos = out.len();
    let n = objects.len() + 1;
    out.push_str(&format!("xref\n0 {n}\n"));
    out.push_str("0000000000 65535 f \n");
    for off in &offsets {
        out.push_str(&format!("{off:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {n} /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n"
    ));
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Segment;

    fn sample() -> Document {
        let mut doc = Document::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(10.0, 0.0), Point2::new(10.0, 8.0))),
        );
        doc
    }

    fn as_str(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn produces_a_structurally_valid_pdf() {
        let pdf = plot(&sample(), PageSize::A4_LANDSCAPE);
        let s = as_str(&pdf);
        assert!(s.starts_with("%PDF-1.7"), "PDF header");
        assert!(s.contains("/Type /Catalog"));
        assert!(s.contains("/Type /Page "));
        assert!(s.contains("stream\n") && s.contains("endstream"));
        assert!(s.contains("\nxref\n") && s.contains("trailer"));
        assert!(s.trim_end().ends_with("%%EOF"));
        // startxref must point at the actual xref keyword.
        let xref_pos = s.find("xref\n0 ").unwrap();
        let claimed: usize = s
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            claimed, xref_pos,
            "startxref offset points at the xref table"
        );
    }

    #[test]
    fn geometry_becomes_stroked_paths() {
        let pdf = as_str(&plot(&sample(), PageSize::A4_LANDSCAPE));
        // Two line entities → two moveto/lineto/stroke sequences.
        assert_eq!(pdf.matches(" m\n").count(), 2);
        assert_eq!(pdf.matches(" l\n").count(), 2);
        assert!(pdf.contains(" RG\n"), "a stroke colour is set");
    }

    #[test]
    fn output_is_byte_deterministic() {
        assert_eq!(
            plot(&sample(), PageSize::A4_LANDSCAPE),
            plot(&sample(), PageSize::A4_LANDSCAPE)
        );
    }

    #[test]
    fn an_empty_drawing_is_a_valid_blank_page() {
        let pdf = as_str(&plot(&Document::new(), PageSize::A4_LANDSCAPE));
        assert!(pdf.starts_with("%PDF-1.7"));
        assert!(pdf.trim_end().ends_with("%%EOF"));
    }
}
