//! # aadt-library — Block, hatch, and font libraries
//!
//! On-disk folders under `libraries/blocks/`, `libraries/hatches/`, and
//! `libraries/fonts/`. Block import accepts DXF block definitions and SVG path
//! geometry; PNG is rejected. Drawing text uses Adobe Source Sans 3 (OFL) from
//! `libraries/fonts/` (also embedded in the desktop shell).

use std::fs;
use std::path::{Path, PathBuf};

use aadt_geometry::{Point2, Polyline, Segment};
use aadt_model::{Block, BlockEntity, Document, Geometry, Hatch};

/// Error loading or importing a library asset.
#[derive(Debug)]
pub enum LibraryError {
    Io(std::io::Error),
    Msg(String),
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LibraryError::Io(e) => write!(f, "{e}"),
            LibraryError::Msg(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        LibraryError::Io(e)
    }
}

impl From<String> for LibraryError {
    fn from(s: String) -> Self {
        LibraryError::Msg(s)
    }
}

/// Resolve the default library root: `libraries/` beside the executable
/// (installed layout), then `cwd/libraries`, then walking up (workspace checkout).
pub fn default_library_root() -> PathBuf {
    // Installed layout: `libraries/` sits next to the binary — the most reliable
    // anchor once the app is installed and launched from an arbitrary cwd.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(cand) = exe.parent().map(|d| d.join("libraries")) {
            if cand.is_dir() {
                return cand;
            }
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local = cwd.join("libraries");
    if local.is_dir() {
        local
    } else {
        // Workspace checkout: walk up looking for libraries/
        let mut p = cwd;
        for _ in 0..6 {
            let cand = p.join("libraries");
            if cand.is_dir() {
                return cand;
            }
            if !p.pop() {
                break;
            }
        }
        PathBuf::from("libraries")
    }
}

/// A discovered block library entry (name + file path).
#[derive(Debug, Clone)]
pub struct BlockEntry {
    pub name: String,
    pub path: PathBuf,
}

/// Scan `root/blocks` for `.dxf` and `.svg` files (recursive; PNG rejected at import).
pub fn list_blocks(root: &Path) -> Result<Vec<BlockEntry>, LibraryError> {
    let dir = root.join("blocks");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    list_blocks_dir(&dir, &mut out)?;
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn list_blocks_dir(dir: &Path, out: &mut Vec<BlockEntry>) -> Result<(), LibraryError> {
    for ent in fs::read_dir(dir)? {
        let ent = ent?;
        let path = ent.path();
        if path.is_dir() {
            list_blocks_dir(&path, out)?;
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "dxf" | "svg") {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("block")
                .to_string();
            out.push(BlockEntry { name, path });
        }
    }
    Ok(())
}

/// Inches → metres (opensource dxfBlocks are drawn in inches).
pub const INCH_TO_METRE: f64 = 0.0254;

/// True when `path` is under `libraries/blocks/opensource` (imperial source units).
pub fn is_opensource_inch_block(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s.eq_ignore_ascii_case("opensource"))
            .unwrap_or(false)
    })
}

/// Import a block file into `doc` as a named block definition.
///
/// - `.dxf` — first block definition in the file, or ENTITIES as an anonymous block
/// - `.svg` — path/`polyline`/`line`/`rect` geometry → polylines
/// - `.png` / raster — rejected
///
/// When `scale` is `None`, opensource inch blocks are auto-scaled by [`INCH_TO_METRE`].
pub fn import_block(
    doc: &mut Document,
    path: &Path,
    name: Option<&str>,
) -> Result<String, LibraryError> {
    let scale = if is_opensource_inch_block(path) {
        Some(INCH_TO_METRE)
    } else {
        None
    };
    import_block_scaled(doc, path, name, scale)
}

/// Import with an explicit uniform scale about the block base (1.0 = leave as-is).
pub fn import_block_scaled(
    doc: &mut Document,
    path: &Path,
    name: Option<&str>,
    scale: Option<f64>,
) -> Result<String, LibraryError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let imported = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => {
            return Err(LibraryError::Msg(format!(
                "raster image {:?} rejected — block library accepts DXF and SVG only",
                path.file_name().unwrap_or_default()
            )))
        }
        "dxf" => import_dxf_block(doc, path, name)?,
        "svg" => import_svg_block(doc, path, name)?,
        other => {
            return Err(LibraryError::Msg(format!(
                "unsupported block format .{other}; use DXF or SVG"
            )))
        }
    };
    if let Some(factor) = scale.filter(|f| (*f - 1.0).abs() > 1e-12) {
        if let Some(block) = doc.block_mut(&imported) {
            let pivot = block.base;
            for e in &mut block.entities {
                e.geometry = e.geometry.scaled(pivot, factor);
            }
            // Nominal WIDTH (metres) from geometry extent for takeoff / scaling.
            if let Some(bb) = block_bbox(block) {
                let w = (bb.max.x - bb.min.x).max(0.0);
                if w > 1e-9 {
                    block.set_param(
                        "WIDTH",
                        format!("{w:.4}"),
                        aadt_model::BlockParamKind::String,
                    );
                }
            }
            block.set_param("UNITS", "m", aadt_model::BlockParamKind::String);
        }
    } else if let Some(block) = doc.block_mut(&imported) {
        if block.param("WIDTH").is_none() {
            if let Some(bb) = block_bbox(block) {
                let w = (bb.max.x - bb.min.x).max(0.0);
                if w > 1e-9 {
                    block.set_param(
                        "WIDTH",
                        format!("{w:.4}"),
                        aadt_model::BlockParamKind::String,
                    );
                }
            }
        }
        if block.param("UNITS").is_none() {
            block.set_param("UNITS", "m", aadt_model::BlockParamKind::String);
        }
    }
    Ok(imported)
}

fn block_bbox(block: &Block) -> Option<aadt_geometry::Bbox> {
    let mut pts = Vec::new();
    for e in &block.entities {
        match &e.geometry {
            Geometry::Line(s) => {
                pts.push(s.a);
                pts.push(s.b);
            }
            Geometry::Polyline(p) => pts.extend(p.vertices.iter().copied()),
            Geometry::Circle(c) => {
                pts.push(Point2::new(c.center.x - c.radius, c.center.y - c.radius));
                pts.push(Point2::new(c.center.x + c.radius, c.center.y + c.radius));
            }
            Geometry::Arc(a) => {
                pts.push(Point2::new(a.center.x - a.radius, a.center.y - a.radius));
                pts.push(Point2::new(a.center.x + a.radius, a.center.y + a.radius));
            }
            Geometry::Text(t) => pts.push(t.position),
            Geometry::Insert(i) => pts.push(i.position),
        }
    }
    aadt_geometry::Bbox::of_points(pts)
}

/// Import every DXF/SVG under `root/blocks` (skips duplicates already in `doc`).
pub fn import_all_blocks(doc: &mut Document, root: &Path) -> Result<Vec<String>, LibraryError> {
    let mut imported = Vec::new();
    for entry in list_blocks(root)? {
        if doc.block(&entry.name).is_some() {
            continue;
        }
        match import_block(doc, &entry.path, Some(&entry.name)) {
            Ok(name) => imported.push(name),
            Err(e) => {
                // Skip broken files; keep going.
                eprintln!("BLOCKLIB skip {}: {e}", entry.path.display());
            }
        }
    }
    Ok(imported)
}

fn import_dxf_block(
    doc: &mut Document,
    path: &Path,
    name: Option<&str>,
) -> Result<String, LibraryError> {
    let src = aadt_dxf::import_path(path).map_err(|e| LibraryError::Msg(e.to_string()))?;
    let block_name = name.map(str::to_string).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("BLOCK")
            .to_string()
    });

    // Prefer a real named BLOCK with geometry. Many library DXFs put the drawing
    // in ENTITIES and only empty `*Model_Space` / `*Paper_Space` in the BLOCKS
    // table — taking `.next()` used to import an empty definition.
    let named = src
        .blocks()
        .into_iter()
        .filter(|b| !b.name.starts_with('*') && !b.entities.is_empty())
        .max_by_key(|b| b.entities.len());
    if let Some(existing) = named {
        let mut b = Block::new(block_name.clone(), existing.base);
        b.entities = existing.entities.clone();
        doc.add_block(b);
        return Ok(block_name);
    }

    // No usable BLOCK table entry — wrap model-space entities as a definition.
    let mut b = Block::new(block_name.clone(), Point2::new(0.0, 0.0));
    for e in src.entities() {
        if matches!(e.geometry, Geometry::Insert(_)) {
            continue;
        }
        b.entities.push(BlockEntity {
            layer: e.layer.clone(),
            geometry: e.geometry.clone(),
        });
    }
    if b.entities.is_empty() {
        return Err(LibraryError::Msg(format!(
            "DXF {:?} has no block definitions or entities",
            path.file_name().unwrap_or_default()
        )));
    }
    doc.add_block(b);
    Ok(block_name)
}

fn import_svg_block(
    doc: &mut Document,
    path: &Path,
    name: Option<&str>,
) -> Result<String, LibraryError> {
    let text = fs::read_to_string(path)?;
    if text.contains("<image")
        || text.to_ascii_lowercase().contains("xlink:href") && text.contains(".png")
    {
        // Embedded raster — reject.
        if text.to_ascii_lowercase().contains(".png")
            || text.contains("data:image/png")
            || text.contains("data:image/jpeg")
        {
            return Err(LibraryError::Msg(
                "SVG embeds a raster image — block library is vector-only".into(),
            ));
        }
    }
    let geoms = parse_svg_geometry(&text)?;
    if geoms.is_empty() {
        return Err(LibraryError::Msg("SVG produced no path geometry".into()));
    }
    let block_name = name.map(str::to_string).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("SVG")
            .to_string()
    });
    let mut b = Block::new(block_name.clone(), Point2::new(0.0, 0.0));
    for g in geoms {
        b.entities.push(BlockEntity {
            layer: "0".into(),
            geometry: g,
        });
    }
    doc.add_block(b);
    Ok(block_name)
}

/// Minimal SVG → geometry: `path d=`, `line`, `polyline`, `polygon`, `rect`.
pub fn parse_svg_geometry(svg: &str) -> Result<Vec<Geometry>, LibraryError> {
    let mut out = Vec::new();
    // path d="..."
    for cap in find_attr_values(svg, "d") {
        out.extend(parse_path_d(&cap)?);
    }
    // polyline / polygon points="..."
    for (tag, closed) in [("polyline", false), ("polygon", true)] {
        for chunk in find_tagged_chunks(svg, tag) {
            if let Some(pts) = find_attr_in(&chunk, "points") {
                let verts = parse_points_list(&pts)?;
                if verts.len() >= 2 {
                    out.push(Geometry::Polyline(Polyline::new(verts, closed)));
                }
            }
        }
    }
    // line x1 y1 x2 y2
    for chunk in find_tagged_chunks(svg, "line") {
        let x1 = attr_f64(&chunk, "x1").unwrap_or(0.0);
        let y1 = attr_f64(&chunk, "y1").unwrap_or(0.0);
        let x2 = attr_f64(&chunk, "x2").unwrap_or(0.0);
        let y2 = attr_f64(&chunk, "y2").unwrap_or(0.0);
        out.push(Geometry::Line(Segment::new(
            Point2::new(x1, y1),
            Point2::new(x2, y2),
        )));
    }
    // rect
    for chunk in find_tagged_chunks(svg, "rect") {
        let x = attr_f64(&chunk, "x").unwrap_or(0.0);
        let y = attr_f64(&chunk, "y").unwrap_or(0.0);
        let w = attr_f64(&chunk, "width").unwrap_or(0.0);
        let h = attr_f64(&chunk, "height").unwrap_or(0.0);
        if w > 0.0 && h > 0.0 {
            out.push(Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(x, y),
                    Point2::new(x + w, y),
                    Point2::new(x + w, y + h),
                    Point2::new(x, y + h),
                ],
                true,
            )));
        }
    }
    Ok(out)
}

fn find_tagged_chunks(svg: &str, tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = svg.to_ascii_lowercase();
    let open = format!("<{tag}");
    let mut start = 0;
    while let Some(i) = lower[start..].find(&open) {
        let abs = start + i;
        let rest = &svg[abs..];
        let end = rest.find('>').unwrap_or(rest.len());
        out.push(rest[..end].to_string());
        start = abs + end + 1;
    }
    out
}

fn find_attr_values(svg: &str, attr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    let key = format!("{attr}=");
    while let Some(i) = rest.find(&key) {
        rest = &rest[i + key.len()..];
        let quote = rest.chars().next().unwrap_or('"');
        if quote == '"' || quote == '\'' {
            rest = &rest[1..];
            if let Some(j) = rest.find(quote) {
                out.push(rest[..j].to_string());
                rest = &rest[j + 1..];
            }
        }
    }
    out
}

fn find_attr_in(chunk: &str, attr: &str) -> Option<String> {
    find_attr_values(chunk, attr).into_iter().next()
}

fn attr_f64(chunk: &str, attr: &str) -> Option<f64> {
    find_attr_in(chunk, attr)?.parse().ok()
}

fn parse_points_list(s: &str) -> Result<Vec<Point2>, LibraryError> {
    let nums: Vec<f64> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.parse::<f64>()
                .map_err(|_| LibraryError::Msg(format!("bad point number {t:?}")))
        })
        .collect::<Result<_, _>>()?;
    if !nums.len().is_multiple_of(2) {
        return Err(LibraryError::Msg("odd number of point coordinates".into()));
    }
    Ok(nums.chunks(2).map(|c| Point2::new(c[0], c[1])).collect())
}

/// Parse a subset of SVG path `d` commands: M/m L/l H/h V/v Z/z.
fn parse_path_d(d: &str) -> Result<Vec<Geometry>, LibraryError> {
    let tokens = tokenize_path(d);
    let mut out = Vec::new();
    let mut verts: Vec<Point2> = Vec::new();
    let mut closed = false;
    let mut i = 0;
    let mut cur = Point2::new(0.0, 0.0);
    let mut cmd = 'M';

    let flush = |verts: &mut Vec<Point2>, closed: &mut bool, out: &mut Vec<Geometry>| {
        if verts.len() >= 2 {
            out.push(Geometry::Polyline(Polyline::new(verts.clone(), *closed)));
        }
        verts.clear();
        *closed = false;
    };

    while i < tokens.len() {
        if tokens[i].len() == 1 && tokens[i].chars().next().unwrap().is_alphabetic() {
            cmd = tokens[i].chars().next().unwrap();
            i += 1;
        }
        match cmd {
            'M' | 'm' => {
                flush(&mut verts, &mut closed, &mut out);
                let (x, y) = read_pair(&tokens, &mut i)?;
                cur = if cmd == 'm' {
                    Point2::new(cur.x + x, cur.y + y)
                } else {
                    Point2::new(x, y)
                };
                verts.push(cur);
                cmd = if cmd == 'm' { 'l' } else { 'L' };
            }
            'L' | 'l' => {
                let (x, y) = read_pair(&tokens, &mut i)?;
                cur = if cmd == 'l' {
                    Point2::new(cur.x + x, cur.y + y)
                } else {
                    Point2::new(x, y)
                };
                verts.push(cur);
            }
            'H' | 'h' => {
                let x = read_num(&tokens, &mut i)?;
                cur = if cmd == 'h' {
                    Point2::new(cur.x + x, cur.y)
                } else {
                    Point2::new(x, cur.y)
                };
                verts.push(cur);
            }
            'V' | 'v' => {
                let y = read_num(&tokens, &mut i)?;
                cur = if cmd == 'v' {
                    Point2::new(cur.x, cur.y + y)
                } else {
                    Point2::new(cur.x, y)
                };
                verts.push(cur);
            }
            'Z' | 'z' => {
                closed = true;
                flush(&mut verts, &mut closed, &mut out);
            }
            other => {
                return Err(LibraryError::Msg(format!(
                    "unsupported SVG path command {other:?} (vector subset: M L H V Z)"
                )));
            }
        }
    }
    flush(&mut verts, &mut closed, &mut out);
    Ok(out)
}

fn tokenize_path(d: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for ch in d.chars() {
        if ch.is_alphabetic() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            tokens.push(ch.to_string());
        } else if ch == ',' || ch.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
        } else if ch == '-' && !cur.is_empty() && !cur.ends_with('e') && !cur.ends_with('E') {
            tokens.push(std::mem::take(&mut cur));
            cur.push(ch);
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn read_num(tokens: &[String], i: &mut usize) -> Result<f64, LibraryError> {
    let t = tokens
        .get(*i)
        .ok_or_else(|| LibraryError::Msg("path: expected number".into()))?;
    *i += 1;
    t.parse()
        .map_err(|_| LibraryError::Msg(format!("path: bad number {t:?}")))
}

fn read_pair(tokens: &[String], i: &mut usize) -> Result<(f64, f64), LibraryError> {
    Ok((read_num(tokens, i)?, read_num(tokens, i)?))
}

// --- Hatch library -----------------------------------------------------------

/// A named hatch pattern available for `HATCH`.
#[derive(Debug, Clone)]
pub struct HatchPattern {
    pub name: String,
    /// Human description.
    pub description: String,
}

/// Built-in patterns always available (no files required).
pub fn builtin_hatch_patterns() -> Vec<HatchPattern> {
    vec![
        HatchPattern {
            name: "SOLID".into(),
            description: "Solid fill".into(),
        },
        HatchPattern {
            name: "ANSI31".into(),
            description: "ANSI iron, brick, stone masonry".into(),
        },
        HatchPattern {
            name: "ANSI32".into(),
            description: "ANSI steel".into(),
        },
        HatchPattern {
            name: "ANSI33".into(),
            description: "ANSI bronze, brass, copper".into(),
        },
        HatchPattern {
            name: "ANSI34".into(),
            description: "ANSI plastic".into(),
        },
        HatchPattern {
            name: "ANSI37".into(),
            description: "ANSI lead, zinc, magnesium / cross-hatch".into(),
        },
        HatchPattern {
            name: "ANSI38".into(),
            description: "ANSI aluminum".into(),
        },
        HatchPattern {
            name: "AR-CONC".into(),
            description: "Architectural concrete".into(),
        },
        HatchPattern {
            name: "AR-BRSTD".into(),
            description: "Brick standard running bond".into(),
        },
        HatchPattern {
            name: "AR-BRDIAG".into(),
            description: "Brick diagonal".into(),
        },
        HatchPattern {
            name: "AR-SAND".into(),
            description: "Sand / aggregate".into(),
        },
        HatchPattern {
            name: "EARTH".into(),
            description: "Earth / soil".into(),
        },
        HatchPattern {
            name: "GRAVEL".into(),
            description: "Gravel".into(),
        },
        HatchPattern {
            name: "STEEL".into(),
            description: "Steel".into(),
        },
        HatchPattern {
            name: "WOOD".into(),
            description: "Wood grain".into(),
        },
        HatchPattern {
            name: "DASH".into(),
            description: "Dashed parallel lines".into(),
        },
        HatchPattern {
            name: "DOTS".into(),
            description: "Dot pattern".into(),
        },
        HatchPattern {
            name: "HONEY".into(),
            description: "Honeycomb / hex".into(),
        },
        HatchPattern {
            name: "NET".into(),
            description: "Square net".into(),
        },
        HatchPattern {
            name: "NET3".into(),
            description: "Triangular net".into(),
        },
        HatchPattern {
            name: "TRIANGLE".into(),
            description: "Triangles".into(),
        },
        HatchPattern {
            name: "ZIGZAG".into(),
            description: "Zigzag".into(),
        },
    ]
}

/// Scan `root/hatches` for `.pat` name stubs (one pattern name per file stem)
/// and merge with builtins.
pub fn list_hatch_patterns(root: &Path) -> Result<Vec<HatchPattern>, LibraryError> {
    let mut out = builtin_hatch_patterns();
    let dir = root.join("hatches");
    if dir.is_dir() {
        for ent in fs::read_dir(&dir)? {
            let ent = ent?;
            let path = ent.path();
            if path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("pat"))
                == Some(true)
            {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("CUSTOM")
                    .to_ascii_uppercase();
                if !out.iter().any(|p| p.name == name) {
                    out.push(HatchPattern {
                        name,
                        description: format!("Custom pattern from {}", path.display()),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Convenience: add a rectangular solid hatch on a layer.
pub fn add_rect_hatch(doc: &mut Document, layer: &str, pattern: &str, min: Point2, max: Point2) {
    let boundary = Polyline::new(
        vec![
            Point2::new(min.x, min.y),
            Point2::new(max.x, min.y),
            Point2::new(max.x, max.y),
            Point2::new(min.x, max.y),
        ],
        true,
    );
    doc.add_hatch(Hatch::new(layer, boundary, pattern));
}

/// Default drawing typeface (Adobe Source Sans 3, SIL OFL).
pub const DEFAULT_DRAWING_FONT: &str = "SourceSans3";

/// A typeface available under `libraries/fonts/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontEntry {
    pub name: String,
    pub path: PathBuf,
}

/// List `.ttf` / `.otf` fonts under `root/fonts`, ensuring Source Sans 3 is first
/// when present so drawings share one open-source sans.
pub fn list_fonts(root: &Path) -> Result<Vec<FontEntry>, LibraryError> {
    let dir = root.join("fonts");
    let mut out = Vec::new();
    if dir.is_dir() {
        for ent in fs::read_dir(&dir)? {
            let ent = ent?;
            let path = ent.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("ttf" | "otf")) {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Font")
                    .to_string();
                out.push(FontEntry { name, path });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    // Prefer Source Sans 3 as the canonical drawing face.
    if let Some(i) = out
        .iter()
        .position(|f| f.name.starts_with("SourceSans3-Regular"))
    {
        let preferred = out.remove(i);
        out.insert(0, preferred);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_png() {
        let mut doc = Document::new();
        let err = import_block(&mut doc, Path::new("door.png"), None).unwrap_err();
        assert!(err.to_string().contains("rejected"));
    }

    #[test]
    fn parses_svg_rect_and_path() {
        let svg =
            r#"<svg><rect x="0" y="0" width="10" height="5"/><path d="M0 0 L10 0 L10 5 Z"/></svg>"#;
        let g = parse_svg_geometry(svg).unwrap();
        assert!(g.len() >= 2);
    }

    #[test]
    fn builtins_include_solid() {
        assert!(builtin_hatch_patterns().iter().any(|p| p.name == "SOLID"));
    }

    #[test]
    fn default_font_constant() {
        assert_eq!(DEFAULT_DRAWING_FONT, "SourceSans3");
    }
}
