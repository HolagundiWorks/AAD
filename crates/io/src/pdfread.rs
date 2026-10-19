//! Reading the vector content of a PDF page, for the take-off underlay.
//!
//! A plan exported from CAD is a PDF of vector paths. This reads those paths (lines, rectangles and curves, with the
//! page's and form XObjects' transforms applied) and returns them as polylines in PDF points, origin at the lower
//! left of the page, so the engine can draw them as ordinary entities that snap and measure. Text, images, shadings
//! and clipping are not drawn; the result counts what was skipped so the caller can say so. A scanned PDF has no
//! paths and is reported as such.
//!
//! A PDF is untrusted input: object depth, operation count, path count and point count are all capped, and every
//! number is checked for finiteness.

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde_json::{Value, json};

use crate::{IoError, Result};

/// Most content operations read from one page (all its forms included).
const MAX_OPS: usize = 4_000_000;
/// Most paths and points returned.
const MAX_PATHS: usize = 200_000;
const MAX_POINTS: usize = 2_000_000;
/// Deepest form XObject nesting.
const MAX_FORM_DEPTH: usize = 8;
/// Straight segments a Bezier curve is cut into.
const CURVE_STEPS: usize = 8;
/// Largest page side accepted, in points.
const MAX_PAGE_PT: f64 = 14_400.0;

#[derive(Clone, Copy)]
struct Matrix([f64; 6]);

impl Matrix {
    const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// `self` applied first, then `then`.
    fn then(self, then: Matrix) -> Matrix {
        let (a, b) = (self.0, then.0);
        Matrix([
            a[0] * b[0] + a[1] * b[2],
            a[0] * b[1] + a[1] * b[3],
            a[2] * b[0] + a[3] * b[2],
            a[2] * b[1] + a[3] * b[3],
            a[4] * b[0] + a[5] * b[2] + b[4],
            a[4] * b[1] + a[5] * b[3] + b[5],
        ])
    }

    fn apply(self, x: f64, y: f64) -> (f64, f64) {
        let m = self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    /// The mean scale of the matrix, for line widths.
    fn scale(self) -> f64 {
        let m = self.0;
        ((m[0] * m[3] - m[1] * m[2]).abs()).sqrt()
    }
}

struct Path {
    points: Vec<(f64, f64)>,
    closed: bool,
}

/// One painted path.
struct Painted {
    points: Vec<(f64, f64)>,
    closed: bool,
    filled: bool,
    stroked: bool,
    rgb: [u8; 3],
    width: f64,
}

#[derive(Default)]
struct Skipped {
    text: usize,
    images: usize,
    shadings: usize,
}

struct Reader<'a> {
    doc: &'a Document,
    out: Vec<Painted>,
    points: usize,
    ops: usize,
    skipped: Skipped,
    truncated: bool,
}

fn num(o: &Object) -> Option<f64> {
    let v = match o {
        Object::Integer(i) => *i as f64,
        Object::Real(r) => f64::from(*r),
        _ => return None,
    };
    v.is_finite().then_some(v)
}

fn nums(args: &[Object]) -> Option<Vec<f64>> {
    args.iter().map(num).collect()
}

fn matrix_of(o: Option<&Object>, doc: &Document) -> Matrix {
    let Some(o) = o else { return Matrix::IDENTITY };
    let Ok((_, o)) = doc.dereference(o) else { return Matrix::IDENTITY };
    let Ok(a) = o.as_array() else { return Matrix::IDENTITY };
    match nums(a) {
        Some(v) if v.len() == 6 => Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]),
        _ => Matrix::IDENTITY,
    }
}

fn gray(g: f64) -> [u8; 3] {
    let v = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
    [v, v, v]
}

fn rgb(r: f64, g: f64, b: f64) -> [u8; 3] {
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [c(r), c(g), c(b)]
}

fn cmyk(c: f64, m: f64, y: f64, k: f64) -> [u8; 3] {
    let f = |v: f64| ((1.0 - v.clamp(0.0, 1.0)) * (1.0 - k.clamp(0.0, 1.0)) * 255.0).round() as u8;
    [f(c), f(m), f(y)]
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    stroke: [u8; 3],
    width: f64,
}

impl<'a> Reader<'a> {
    fn run(&mut self, content: &[u8], resources: Option<&Dictionary>, base: Matrix, depth: usize) {
        let Ok(content) = Content::decode(content) else { return };
        let mut st = State { ctm: base, stroke: [0, 0, 0], width: 1.0 };
        let mut stack: Vec<State> = Vec::new();
        let mut paths: Vec<Path> = Vec::new();
        let mut cur: Vec<(f64, f64)> = Vec::new();
        for op in &content.operations {
            self.ops += 1;
            if self.ops > MAX_OPS || self.out.len() >= MAX_PATHS || self.points >= MAX_POINTS {
                self.truncated = true;
                return;
            }
            let a = &op.operands;
            let at = |ctm: Matrix, x: f64, y: f64| ctm.apply(x, y);
            match op.operator.as_str() {
                "q" => {
                    if stack.len() < 64 {
                        stack.push(st.clone());
                    }
                }
                "Q" => {
                    if let Some(s) = stack.pop() {
                        st = s;
                    }
                }
                "cm" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 6) {
                        st.ctm = Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]).then(st.ctm);
                    }
                }
                "w" => {
                    if let Some(v) = a.first().and_then(num) {
                        st.width = v.abs();
                    }
                }
                "G" => {
                    if let Some(v) = a.first().and_then(num) {
                        st.stroke = gray(v);
                    }
                }
                "RG" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 3) {
                        st.stroke = rgb(v[0], v[1], v[2]);
                    }
                }
                "K" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 4) {
                        st.stroke = cmyk(v[0], v[1], v[2], v[3]);
                    }
                }
                "m" => {
                    flush(&mut paths, &mut cur, false);
                    if let Some(v) = nums(a).filter(|v| v.len() == 2) {
                        cur.push(at(st.ctm, v[0], v[1]));
                    }
                }
                "l" => {
                    if let Some(v) = nums(a).filter(|v| v.len() == 2) {
                        cur.push(at(st.ctm, v[0], v[1]));
                    }
                }
                "c" | "v" | "y" => {
                    let Some(v) = nums(a) else { continue };
                    let Some(&p0) = cur.last() else { continue };
                    let pt = |x: f64, y: f64| at(st.ctm, x, y);
                    let (p1, p2, p3) = match (op.operator.as_str(), v.len()) {
                        ("c", 6) => (pt(v[0], v[1]), pt(v[2], v[3]), pt(v[4], v[5])),
                        ("v", 4) => (p0, pt(v[0], v[1]), pt(v[2], v[3])),
                        ("y", 4) => (pt(v[0], v[1]), pt(v[2], v[3]), pt(v[2], v[3])),
                        _ => continue,
                    };
                    for i in 1..=CURVE_STEPS {
                        let t = i as f64 / CURVE_STEPS as f64;
                        let u = 1.0 - t;
                        let x = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
                        let y = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
                        cur.push((x, y));
                    }
                }
                "re" => {
                    flush(&mut paths, &mut cur, false);
                    if let Some(v) = nums(a).filter(|v| v.len() == 4) {
                        let (x, y, w, h) = (v[0], v[1], v[2], v[3]);
                        let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)].map(|(px, py)| at(st.ctm, px, py));
                        paths.push(Path { points: pts.to_vec(), closed: true });
                    }
                }
                "h" => {
                    if !cur.is_empty() {
                        flush(&mut paths, &mut cur, true);
                    }
                }
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                    let close = matches!(op.operator.as_str(), "s" | "b" | "b*");
                    flush(&mut paths, &mut cur, close);
                    let (stroked, filled) = match op.operator.as_str() {
                        "S" | "s" => (true, false),
                        "f" | "F" | "f*" => (false, true),
                        "B" | "B*" | "b" | "b*" => (true, true),
                        _ => (false, false),
                    };
                    if stroked || filled {
                        for p in paths.drain(..) {
                            if p.points.len() < 2 || self.points + p.points.len() > MAX_POINTS {
                                continue;
                            }
                            self.points += p.points.len();
                            self.out.push(Painted {
                                points: p.points,
                                closed: p.closed,
                                filled,
                                stroked,
                                rgb: st.stroke,
                                width: st.width * st.ctm.scale(),
                            });
                        }
                    } else {
                        paths.clear();
                    }
                }
                "Tj" | "TJ" | "'" | "\"" => self.skipped.text += 1,
                "BI" => self.skipped.images += 1,
                "sh" => self.skipped.shadings += 1,
                "Do" => {
                    let Some(name) = a.first().and_then(|o| o.as_name().ok()) else { continue };
                    self.xobject(resources, name, st.ctm, depth);
                }
                _ => {}
            }
        }
    }

    fn xobject(&mut self, resources: Option<&Dictionary>, name: &[u8], ctm: Matrix, depth: usize) {
        let Some(res) = resources else { return };
        let Some(xo) = res.get(b"XObject").ok().and_then(|o| self.doc.dereference(o).ok()).and_then(|(_, o)| o.as_dict().ok()) else { return };
        let Some(entry) = xo.get(name).ok() else { return };
        let Ok((_, obj)) = self.doc.dereference(entry) else { return };
        let Ok(stream) = obj.as_stream() else { return };
        match stream.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) {
            Some(b"Image") => self.skipped.images += 1,
            Some(b"Form") if depth < MAX_FORM_DEPTH => {
                let Ok(data) = stream.decompressed_content() else { return };
                let m = matrix_of(stream.dict.get(b"Matrix").ok(), self.doc).then(ctm);
                let own = stream.dict.get(b"Resources").ok().and_then(|o| self.doc.dereference(o).ok()).and_then(|(_, o)| o.as_dict().ok());
                self.run(&data, own.or(resources), m, depth + 1);
            }
            _ => {}
        }
    }
}

fn flush(paths: &mut Vec<Path>, cur: &mut Vec<(f64, f64)>, closed: bool) {
    if cur.len() >= 2 {
        paths.push(Path { points: std::mem::take(cur), closed });
    } else {
        cur.clear();
    }
}

/// The page box (inherited through `Parent`), its rotation, in points.
fn page_box(doc: &Document, page: ObjectId) -> Option<([f64; 4], i64)> {
    let mut id = page;
    let mut rotate = None;
    for _ in 0..32 {
        let d = doc.get_dictionary(id).ok()?;
        if rotate.is_none() {
            rotate = d.get(b"Rotate").ok().and_then(|o| doc.dereference(o).ok()).and_then(|(_, o)| o.as_i64().ok());
        }
        if let Some(b) = d.get(b"MediaBox").ok().and_then(|o| doc.dereference(o).ok()).and_then(|(_, o)| o.as_array().ok()) {
            let v = nums(b).filter(|v| v.len() == 4)?;
            return Some(([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])], rotate.unwrap_or(0)));
        }
        id = d.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// Read page `page` (from 1) of a PDF. The result is JSON:
/// `{pages, width_pt, height_pt, paths: [{pts: [[x, y]...], closed, filled, stroked, rgb, width_pt}], skipped: {text, images, shadings}, truncated}`.
pub fn read_page(bytes: &[u8], page: u32) -> Result<Value> {
    let doc = Document::load_mem(bytes).map_err(|e| IoError::Format(format!("not a readable PDF: {e}")))?;
    if doc.is_encrypted() {
        return Err(IoError::Format("the PDF is encrypted; save an unprotected copy first".into()));
    }
    let pages = doc.get_pages();
    let id = *pages.get(&page).ok_or_else(|| IoError::Format(format!("the PDF has {} page(s); there is no page {page}", pages.len())))?;
    let ([x0, y0, x1, y1], rotate) = page_box(&doc, id).ok_or_else(|| IoError::Format("the page has no usable MediaBox".into()))?;
    let (w, h) = (x1 - x0, y1 - y0);
    if !(w > 0.0 && h > 0.0 && w <= MAX_PAGE_PT && h <= MAX_PAGE_PT) {
        return Err(IoError::Format("the page size is out of range".into()));
    }
    // Origin at the lower left of the page box; a page rotation turns the page the way a viewer shows it.
    let origin = Matrix([1.0, 0.0, 0.0, 1.0, -x0, -y0]);
    let (base, width, height) = match rotate.rem_euclid(360) {
        90 => (origin.then(Matrix([0.0, -1.0, 1.0, 0.0, 0.0, w])), h, w),
        180 => (origin.then(Matrix([-1.0, 0.0, 0.0, -1.0, w, h])), w, h),
        270 => (origin.then(Matrix([0.0, 1.0, -1.0, 0.0, h, 0.0])), h, w),
        _ => (origin, w, h),
    };
    let content = doc.get_page_content(id);
    let resources = doc.get_page_resources(id).ok().and_then(|(r, _)| r);
    let mut reader = Reader { doc: &doc, out: Vec::new(), points: 0, ops: 0, skipped: Skipped::default(), truncated: false };
    reader.run(&content, resources, base, 0);
    let paths: Vec<Value> = reader
        .out
        .iter()
        .map(|p| {
            let pts: Vec<Value> = p.points.iter().map(|(x, y)| json!([x, y])).collect();
            json!({ "pts": pts, "closed": p.closed, "filled": p.filled, "stroked": p.stroked, "rgb": p.rgb, "width_pt": p.width })
        })
        .collect();
    Ok(json!({
        "pages": pages.len(), "width_pt": width, "height_pt": height, "paths": paths,
        "skipped": { "text": reader.skipped.text, "images": reader.skipped.images, "shadings": reader.skipped.shadings },
        "truncated": reader.truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page PDF made by hand: a stroked line, a rectangle and a filled triangle under a translation.
    fn sample() -> Vec<u8> {
        let content = "0 0 1 RG 2 w 10 10 m 110 10 l S\n20 20 50 30 re S\nq 1 0 0 1 100 100 cm 0 0 m 40 0 l 20 30 l f Q\nBT (hello) Tj ET";
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 150] /Contents 4 0 R /Resources << >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len() + 1),
        ];
        for (i, o) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1));
        for o in &offsets {
            pdf.push_str(&format!("{o:010} 00000 n \n"));
        }
        pdf.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1));
        pdf.into_bytes()
    }

    #[test]
    fn a_hand_made_page_gives_its_paths_in_points() {
        let r = read_page(&sample(), 1).unwrap();
        assert_eq!((r["width_pt"].as_f64(), r["height_pt"].as_f64(), r["pages"].as_u64()), (Some(200.0), Some(150.0), Some(1)));
        let paths = r["paths"].as_array().unwrap();
        assert_eq!(paths.len(), 3, "{r}");
        assert_eq!(paths[0]["pts"], json!([[10.0, 10.0], [110.0, 10.0]]));
        assert_eq!(paths[0]["rgb"], json!([0, 0, 255]));
        assert_eq!(paths[1]["closed"], true);
        assert_eq!(paths[1]["pts"].as_array().unwrap().len(), 4);
        // The triangle sits under `1 0 0 1 100 100 cm`.
        assert_eq!(paths[2]["pts"][0], json!([100.0, 100.0]));
        assert_eq!(paths[2]["filled"], true);
        assert_eq!(r["skipped"]["text"], 1, "the text is counted, not drawn");
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(read_page(b"", 1).is_err());
        assert!(read_page(b"%PDF-1.4 nonsense", 1).is_err());
        assert!(read_page(&sample(), 2).is_err(), "no page 2");
        assert!(read_page(&sample(), 0).is_err());
        let mut cut = sample();
        cut.truncate(cut.len() / 2);
        let _ = read_page(&cut, 1);
    }

    /// A PDF whose form XObject draws itself: the nesting cap ends it, quickly, with what was drawn on the way.
    #[test]
    fn a_form_that_draws_itself_ends() {
        let page = "/Fm0 Do 0 0 m 10 10 l S";
        let form = "0 0 m 5 5 l S /Fm0 Do";
        let res = "<< /XObject << /Fm0 5 0 R >> >>";
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R /Resources {res} >>"),
            format!("<< /Length {} >>\nstream\n{page}\nendstream", page.len() + 1),
            format!("<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources {res} /Length {} >>\nstream\n{form}\nendstream", form.len() + 1),
        ];
        for (i, o) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1));
        for o in &offsets {
            pdf.push_str(&format!("{o:010} 00000 n \n"));
        }
        pdf.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1));
        let r = read_page(pdf.as_bytes(), 1).unwrap();
        let n = r["paths"].as_array().unwrap().len();
        assert!((2..=20).contains(&n), "the recursion is cut at the depth cap: {n} paths");
    }
}
