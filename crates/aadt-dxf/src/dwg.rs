//! DWG import/export via open-source [`acadrust`] (MPL-2.0). See ADR-0002.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
use aadt_model::{
    Block, Dimension, DimensionKind, Document, Geometry, Hatch, ImageRef, Insert, Layer, Text,
};
use acadrust::entities::{
    Arc as AcadArc, BoundaryEdge, BoundaryPath, BoundaryPathFlags, Circle as AcadCircle,
    Dimension as AcadDim, DimensionAligned, DimensionAngular3Pt, DimensionDiameter,
    DimensionLinear, DimensionOrdinate, DimensionRadius, EntityCommon, EntityType,
    Hatch as AcadHatch, HatchPattern, Insert as AcadInsert, Line as AcadLine, LwPolyline,
    PolylineEdge, RasterImage, Text as AcadText,
};
use acadrust::io::dwg::{DwgReader, DwgWriter};
use acadrust::objects::{Dictionary, ImageDefinition, ImageDefinitionReactor, ObjectType};
use acadrust::tables::{BlockRecord, Layer as AcadLayer, TableEntry};
use acadrust::types::{Color, Handle, LineWeight, Vector2, Vector3};
use acadrust::{CadDocument, Entity};

use crate::DxfError;

/// Read a DWG file into a fresh [`Document`] (supported 2D entity subset).
pub fn import_dwg_path(path: impl AsRef<Path>) -> Result<Document, DxfError> {
    let mut reader = DwgReader::from_file(path.as_ref()).map_err(map_acad)?;
    let cad = reader.read().map_err(map_acad)?;
    Ok(from_cad_document(&cad))
}

/// Write a [`Document`] to a DWG file (best-effort 2D subset).
///
/// Raster images are written as AutoCAD `IMAGE` + `IMAGEDEF` (+ reactor).
/// Hatches and common dimension kinds are written when acadrust can encode them
/// (L4). Returns human-readable fidelity warnings for skipped items.
///
/// Inserts whose block is missing are skipped with a warning.
pub fn export_dwg_path(doc: &Document, path: impl AsRef<Path>) -> Result<Vec<String>, DxfError> {
    let (cad, warnings) = to_cad_document(doc)?;
    DwgWriter::write_to_file(path.as_ref(), &cad).map_err(map_acad)?;
    Ok(warnings)
}

/// Build an [`acadrust::CadDocument`] from an AADT [`Document`].
///
/// Used by DWG export and by DXF export when raster images need a real
/// `IMAGEDEF` / `OBJECTS` section (hand-rolled ASCII DXF has no handles).
pub fn to_cad_document(doc: &Document) -> Result<(CadDocument, Vec<String>), DxfError> {
    to_cad_document_inner(doc)
}

fn map_acad(e: acadrust::DxfError) -> DxfError {
    DxfError::Parse(format!("DWG: {e}"))
}

fn layer_name(common: &EntityCommon) -> String {
    if common.layer.is_empty() {
        "0".into()
    } else {
        common.layer.clone()
    }
}

fn ensure_layer(doc: &mut Document, name: &str) {
    if doc.layer(name).is_none() {
        doc.add_layer(Layer::new(name));
    }
}

fn with_layer(mut common: EntityCommon, layer: &str) -> EntityCommon {
    common.layer = if layer.is_empty() {
        "0".into()
    } else {
        layer.to_string()
    };
    common
}

fn owner_of(ent: &EntityType) -> Handle {
    ent.common().owner_handle
}

fn is_model_space_entity(cad: &CadDocument, ent: &EntityType) -> bool {
    let ms = cad.header.model_space_block_handle;
    let ps = cad.header.paper_space_block_handle;
    let owner = owner_of(ent);
    if owner.is_null() {
        return true;
    }
    if !ps.is_null() && owner == ps {
        return false;
    }
    if !ms.is_null() {
        return owner == ms;
    }
    true
}

fn from_cad_document(cad: &CadDocument) -> Document {
    let mut doc = Document::new();
    let mut skipped = 0usize;

    // Named block definitions (skip *Model_Space / *Paper_Space / anonymous *U*).
    for br in cad.block_records.iter() {
        let name = br.name.as_str();
        if name.starts_with('*') {
            continue;
        }
        let mut block = Block::new(name, Point2::new(br.base_point.x, br.base_point.y));
        for h in &br.entity_handles {
            let Some(ent) = cad.get_entity(*h) else {
                continue;
            };
            match convert_entity(cad, ent) {
                Some((layer, geom)) => block.push(layer, geom),
                None => skipped += 1,
            }
        }
        if !block.entities.is_empty() || doc.block(name).is_none() {
            doc.add_block(block);
        }
    }

    for ent in cad.entities() {
        if let EntityType::Hatch(h) = ent {
            // Prefer model-space hatches; still accept orphans (null owner).
            if !is_model_space_entity(cad, ent) && !owner_of(ent).is_null() {
                skipped += 1;
                continue;
            }
            if let Some(hatch) = convert_hatch(h) {
                ensure_layer(&mut doc, &hatch.layer);
                doc.add_hatch(hatch);
            } else {
                skipped += 1;
            }
            continue;
        }
        if let EntityType::Dimension(d) = ent {
            if !is_model_space_entity(cad, ent) {
                continue;
            }
            let base = d.base();
            let layer = layer_name(&base.common);
            ensure_layer(&mut doc, &layer);
            // Prefer the anonymous *D dimension block (exact CAD graphics + text).
            let exploded = explode_named_block(cad, &mut doc, &base.block_name, &layer);
            if let Some(dim) = convert_dimension(d) {
                // Only keep the parametric dimension when we could not explode
                // visuals — otherwise the draw list would paint twice.
                if !exploded {
                    doc.add_dimension(dim);
                }
            } else if !exploded {
                skipped += 1;
            }
            continue;
        }
        if !is_model_space_entity(cad, ent) {
            continue;
        }
        match convert_entity(cad, ent) {
            Some((layer, geom)) => {
                ensure_layer(&mut doc, &layer);
                doc.add_geometry(layer, geom);
            }
            None => skipped += 1,
        }
    }
    if skipped > 0 {
        let mut props = doc.properties().clone();
        if !props.comments.is_empty() {
            props.comments.push(' ');
        }
        props.comments.push_str(&format!(
            "[DWG import skipped {skipped} unsupported entit(y/ies)]"
        ));
        doc.set_properties(props);
    }
    if !cad.block_visibility_params.is_empty() {
        let mut props = doc.properties().clone();
        if !props.comments.is_empty() {
            props.comments.push(' ');
        }
        props
            .comments
            .push_str("[DWG: dynamic actions not editable in AADT]");
        doc.set_properties(props);
    }
    doc
}

/// Build an acadrust LAYER table entry from an AADT layer definition, so the
/// exported DWG carries real layer records (colour / lineweight / frozen /
/// locked) instead of leaving entities pointing at an undefined layer.
fn to_cad_layer(doc: &Document, name: &str) -> AcadLayer {
    let mut al = AcadLayer::new(name);
    if let Some(l) = doc.layer(name) {
        al.color = Color::from_index(l.color as i16);
        al.line_weight = if l.lineweight_mm > 0.0 {
            // DWG lineweights are hundredths of a millimetre.
            LineWeight::from_value((l.lineweight_mm * 100.0).round() as i16)
        } else {
            LineWeight::Default
        };
        al.is_plottable = true;
        if l.frozen {
            al.freeze();
        }
        if l.locked {
            al.lock();
        }
    }
    al
}

fn to_cad_document_inner(doc: &Document) -> Result<(CadDocument, Vec<String>), DxfError> {
    let mut cad = CadDocument::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    // Layers referenced by the entities we actually export; "0" always exists.
    let mut used: BTreeSet<String> = BTreeSet::new();
    // path → (definition handle, pixel width, pixel height)
    let mut image_defs: HashMap<String, (Handle, u32, u32)> = HashMap::new();

    // Named block definitions first so INSERT references resolve.
    for block in doc.blocks() {
        if block.name.starts_with('*') {
            continue;
        }
        let mut br = BlockRecord::new(&block.name);
        br.set_handle(cad.allocate_handle());
        br.block_entity_handle = cad.allocate_handle();
        br.block_end_handle = cad.allocate_handle();
        br.base_point = Vector3::new(block.base.x, block.base.y, 0.0);
        let br_handle = br.handle;
        if cad.block_records.add(br).is_err() {
            skipped += 1;
            warnings.push(format!("skipped block def '{}'", block.name));
            continue;
        }
        for be in &block.entities {
            if let Geometry::Image(img) = &be.geometry {
                match add_image_to_cad(
                    &mut cad,
                    &be.layer,
                    img,
                    br_handle,
                    &mut image_defs,
                    &mut warnings,
                ) {
                    Ok(()) => {
                        used.insert(if be.layer.is_empty() {
                            "0".to_string()
                        } else {
                            be.layer.clone()
                        });
                    }
                    Err(()) => skipped += 1,
                }
                continue;
            }
            let Some(mut ent) = geometry_to_entity(&be.layer, &be.geometry) else {
                skipped += 1;
                continue;
            };
            ent.common_mut().owner_handle = br_handle;
            if cad.add_entity(ent).is_err() {
                skipped += 1;
            } else {
                used.insert(if be.layer.is_empty() {
                    "0".to_string()
                } else {
                    be.layer.clone()
                });
            }
        }
    }

    for e in doc.entities() {
        // Skip inserts whose block definition is missing (silent when possible).
        if let Geometry::Insert(ins) = &e.geometry {
            if doc.block(&ins.name).is_none() {
                let msg = format!(
                    "skipped INSERT '{}' — block definition missing",
                    ins.name
                );
                eprintln!("aadt-dxf: {msg}");
                warnings.push(msg);
                skipped += 1;
                continue;
            }
        }
        if let Geometry::Image(img) = &e.geometry {
            match add_image_to_cad(
                &mut cad,
                &e.layer,
                img,
                Handle::NULL, // model space via add_entity default
                &mut image_defs,
                &mut warnings,
            ) {
                Ok(()) => {
                    used.insert(if e.layer.is_empty() {
                        "0".to_string()
                    } else {
                        e.layer.clone()
                    });
                }
                Err(()) => skipped += 1,
            }
            continue;
        }
        let g = match &e.geometry {
            Geometry::Insert(ins) => {
                let flipped = e
                    .attribute(aadt_model::BLOCK_FLIP_ATTR)
                    .or_else(|| e.attribute(aadt_model::BLOCK_FLIP_PARAM))
                    .map(|v| {
                        v == "1"
                            || v.eq_ignore_ascii_case("true")
                            || v.eq_ignore_ascii_case("yes")
                    })
                    .unwrap_or(false);
                let mut baked = ins.clone();
                if flipped && baked.scale_x > 0.0 {
                    baked.scale_x = -baked.scale_x;
                }
                Geometry::Insert(baked)
            }
            other => other.clone(),
        };
        match geometry_to_entity(&e.layer, &g) {
            Some(ent) => {
                if cad.add_entity(ent).is_err() {
                    skipped += 1;
                } else {
                    used.insert(if e.layer.is_empty() {
                        "0".to_string()
                    } else {
                        e.layer.clone()
                    });
                }
            }
            None => skipped += 1,
        }
    }
    // Write a LAYER table record for every referenced layer (skip the built-in
    // "0", which CadDocument::new already defines with a handle).
    for name in &used {
        if cad.layers.get(name).is_some() {
            continue;
        }
        let mut al = to_cad_layer(doc, name);
        al.set_handle(cad.allocate_handle());
        cad.layers.add_or_replace(al);
    }

    // L4 — hatches (multi-path polyline boundaries).
    let mut hatch_fail = 0usize;
    for h in doc.hatches() {
        match hatch_to_entity(h) {
            Some(ent) => {
                if cad.add_entity(ent).is_err() {
                    hatch_fail += 1;
                    skipped += 1;
                } else {
                    used.insert(if h.layer.is_empty() {
                        "0".to_string()
                    } else {
                        h.layer.clone()
                    });
                }
            }
            None => {
                hatch_fail += 1;
                skipped += 1;
            }
        }
    }
    if hatch_fail > 0 {
        warnings.push(format!(
            "skipped {hatch_fail} hatch(es) — empty/unsupported boundary"
        ));
    }

    // L4 — dimensions (aligned / linear / radius / diameter / angular / ordinate).
    let mut dim_fail = 0usize;
    for d in doc.dimensions() {
        match dimension_to_entity(d) {
            Some(ent) => {
                if cad.add_entity(ent).is_err() {
                    dim_fail += 1;
                    skipped += 1;
                } else {
                    used.insert(if d.layer.is_empty() {
                        "0".to_string()
                    } else {
                        d.layer.clone()
                    });
                }
            }
            None => {
                dim_fail += 1;
                skipped += 1;
            }
        }
    }
    if dim_fail > 0 {
        warnings.push(format!(
            "skipped {dim_fail} dimension(s) — unsupported kind or empty geometry"
        ));
    }

    // Layers referenced only by hatches/dims (added after the first layer pass).
    for name in &used {
        if cad.layers.get(name).is_some() {
            continue;
        }
        let mut al = to_cad_layer(doc, name);
        al.set_handle(cad.allocate_handle());
        cad.layers.add_or_replace(al);
    }

    if skipped > 0 && warnings.is_empty() {
        warnings.push(format!("DWG export skipped {skipped} item(s)"));
    }
    warnings.sort();
    warnings.dedup();
    Ok((cad, warnings))
}

/// Probe raster pixel size; fall back to a 1024-wide estimate from world aspect.
fn image_pixel_size(path: &str, world_w: f64, world_h: f64) -> (u32, u32) {
    if let Ok((w, h)) = image::image_dimensions(Path::new(path)) {
        return (w.max(1), h.max(1));
    }
    let aspect = (world_w / world_h.max(1e-9)).abs().max(1e-6);
    let w = 1024u32;
    let h = ((w as f64 / aspect).round() as u32).max(1);
    (w, h)
}

fn ensure_acad_image_dict(cad: &mut CadDocument) -> Handle {
    let root_h = cad.header.named_objects_dict_handle;
    if let Some(ObjectType::Dictionary(root)) = cad.objects.get(&root_h) {
        if let Some(h) = root.get("ACAD_IMAGE_DICT") {
            return h;
        }
    }
    let img_dict_h = cad.allocate_handle();
    let mut img_dict = Dictionary::new();
    img_dict.handle = img_dict_h;
    img_dict.owner = root_h;
    cad.objects
        .insert(img_dict_h, ObjectType::Dictionary(img_dict));
    if let Some(ObjectType::Dictionary(root)) = cad.objects.get_mut(&root_h) {
        root.add_entry("ACAD_IMAGE_DICT", img_dict_h);
    }
    img_dict_h
}

fn ensure_image_definition(
    cad: &mut CadDocument,
    path: &str,
    world_w: f64,
    world_h: f64,
    defs: &mut HashMap<String, (Handle, u32, u32)>,
    warnings: &mut Vec<String>,
) -> (Handle, u32, u32) {
    if let Some(&(h, pw, ph)) = defs.get(path) {
        return (h, pw, ph);
    }
    let (px_w, px_h) = image_pixel_size(path, world_w, world_h);
    if !Path::new(path).is_file() {
        warnings.push(format!(
            "IMAGEDEF '{path}' — file not found beside export; AutoCAD may show a blank frame"
        ));
    }
    let img_dict_h = ensure_acad_image_dict(cad);
    let def_h = cad.allocate_handle();
    let pixel_w = world_w / px_w as f64;
    let pixel_h = world_h / px_h as f64;
    let mut def = ImageDefinition::with_dimensions(path, px_w, px_h)
        .with_pixel_size(pixel_w, pixel_h)
        .loaded();
    def.handle = def_h;
    def.owner = img_dict_h;
    // Unique dictionary key (file stem, uniquified).
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("IMAGE");
    let mut key = stem.to_string();
    let mut n = 1u32;
    if let Some(ObjectType::Dictionary(dict)) = cad.objects.get(&img_dict_h) {
        while dict.get(&key).is_some() {
            n += 1;
            key = format!("{stem}_{n}");
        }
    }
    if let Some(ObjectType::Dictionary(dict)) = cad.objects.get_mut(&img_dict_h) {
        dict.add_entry(key, def_h);
    }
    cad.objects
        .insert(def_h, ObjectType::ImageDefinition(def));
    defs.insert(path.to_string(), (def_h, px_w, px_h));
    (def_h, px_w, px_h)
}

fn add_image_to_cad(
    cad: &mut CadDocument,
    layer: &str,
    img: &ImageRef,
    owner: Handle,
    defs: &mut HashMap<String, (Handle, u32, u32)>,
    warnings: &mut Vec<String>,
) -> Result<(), ()> {
    if img.path.trim().is_empty() || img.width <= 0.0 || img.height <= 0.0 {
        return Err(());
    }
    let (def_h, px_w, px_h) =
        ensure_image_definition(cad, &img.path, img.width, img.height, defs, warnings);
    let mut ri = RasterImage::with_size(
        &img.path,
        Vector3::new(img.position.x, img.position.y, 0.0),
        px_w as f64,
        px_h as f64,
        img.width,
        img.height,
    );
    if img.rotation.abs() > 1e-12 {
        ri.rotate(img.rotation.to_radians());
    }
    if img.opacity < 0.999 {
        ri.set_transparent(true);
        ri.fade = ((1.0 - img.opacity as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8;
    }
    ri.common = with_layer(EntityCommon::new(), layer);
    if !owner.is_null() {
        ri.common.owner_handle = owner;
    }
    ri.definition_handle = Some(def_h);

    let ent_h = cad
        .add_entity(EntityType::RasterImage(ri))
        .map_err(|_| ())?;

    let mut reactor = ImageDefinitionReactor::new(ent_h);
    reactor.handle = cad.allocate_handle();
    reactor.owner = def_h;
    let reactor_h = reactor.handle;
    cad.objects
        .insert(reactor_h, ObjectType::ImageDefinitionReactor(reactor));
    if let Some(EntityType::RasterImage(ri)) = cad.get_entity_mut(ent_h) {
        ri.definition_reactor_handle = Some(reactor_h);
    }
    Ok(())
}

fn geometry_to_entity(layer: &str, g: &Geometry) -> Option<EntityType> {
    match g {
        Geometry::Line(s) => {
            let mut line = AcadLine::from_coords(s.a.x, s.a.y, 0.0, s.b.x, s.b.y, 0.0);
            line.common = with_layer(EntityCommon::new(), layer);
            Some(EntityType::Line(line))
        }
        Geometry::Circle(c) => {
            let mut circle =
                AcadCircle::from_center_radius(Vector3::new(c.center.x, c.center.y, 0.0), c.radius);
            circle.common = with_layer(EntityCommon::new(), layer);
            Some(EntityType::Circle(circle))
        }
        Geometry::Arc(a) => {
            let mut arc = AcadArc::from_coords(
                a.center.x,
                a.center.y,
                0.0,
                a.radius,
                a.start_angle.to_radians(),
                a.end_angle.to_radians(),
            );
            arc.common = with_layer(EntityCommon::new(), layer);
            Some(EntityType::Arc(arc))
        }
        Geometry::Polyline(p) => {
            if p.vertices.len() < 2 {
                return None;
            }
            let pts: Vec<Vector2> = p.vertices.iter().map(|v| Vector2::new(v.x, v.y)).collect();
            let mut pl = LwPolyline::from_points(pts);
            pl.is_closed = p.closed;
            pl.common = with_layer(EntityCommon::new(), layer);
            Some(EntityType::LwPolyline(pl))
        }
        Geometry::Text(t) => {
            let mut text = AcadText::with_value(
                t.value.clone(),
                Vector3::new(t.position.x, t.position.y, 0.0),
            )
            .with_height(t.height.max(0.01))
            .with_rotation(t.rotation.to_radians());
            text.common = with_layer(EntityCommon::new(), layer);
            Some(EntityType::Text(text))
        }
        Geometry::Insert(ins) => {
            let mut insert = AcadInsert::new(
                ins.name.clone(),
                Vector3::new(ins.position.x, ins.position.y, 0.0),
            );
            insert.rotation = ins.rotation.to_radians();
            insert.set_x_scale(ins.scale_x);
            insert.set_y_scale(ins.scale_y);
            insert.common = with_layer(EntityCommon::new(), layer);
            Some(EntityType::Insert(insert))
        }
        // Images are registered via IMAGEDEF in `add_image_to_cad`.
        Geometry::Image(_) => None,
    }
}

fn p3(p: Point2) -> Vector3 {
    Vector3::new(p.x, p.y, 0.0)
}

/// Map an AADT hatch to an acadrust `HATCH` (polyline boundary paths).
fn hatch_to_entity(h: &Hatch) -> Option<EntityType> {
    if h.boundaries.is_empty() {
        return None;
    }
    let mut acad = if h.solid || h.pattern.eq_ignore_ascii_case("SOLID") {
        AcadHatch::solid()
    } else {
        AcadHatch::with_pattern(HatchPattern::new(h.pattern.clone()))
    };
    acad.common = with_layer(EntityCommon::new(), &h.layer);
    for (i, pl) in h.boundaries.iter().enumerate() {
        if pl.vertices.len() < 3 {
            continue;
        }
        let verts: Vec<Vector2> = pl.vertices.iter().map(|v| Vector2::new(v.x, v.y)).collect();
        let edge = PolylineEdge::new(verts, true);
        let mut flags = BoundaryPathFlags::from_bits(
            BoundaryPathFlags::EXTERNAL.bits() | BoundaryPathFlags::POLYLINE.bits(),
        );
        if i == 0 {
            flags = BoundaryPathFlags::from_bits(flags.bits() | BoundaryPathFlags::OUTERMOST.bits());
        }
        let mut path = BoundaryPath::with_flags(flags);
        path.add_edge(BoundaryEdge::Polyline(edge));
        acad.add_path(path);
    }
    if !acad.has_paths() {
        return None;
    }
    // Seed near the outer centroid so viewers have a fill pick point.
    if let Some(outer) = h.boundaries.first() {
        let n = outer.vertices.len() as f64;
        if n > 0.0 {
            let sx: f64 = outer.vertices.iter().map(|v| v.x).sum();
            let sy: f64 = outer.vertices.iter().map(|v| v.y).sum();
            acad.add_seed_point(Vector2::new(sx / n, sy / n));
        }
    }
    Some(EntityType::Hatch(acad))
}

/// Map an AADT dimension to an acadrust dimension entity.
fn dimension_to_entity(d: &Dimension) -> Option<EntityType> {
    let mut acad = match d.kind {
        DimensionKind::Aligned => {
            let mut a = DimensionAligned::new(p3(d.p1), p3(d.p2));
            a.definition_point = p3(d.vertex);
            if d.measurement.abs() > f64::EPSILON {
                a.base.actual_measurement = d.measurement;
            }
            AcadDim::Aligned(a)
        }
        DimensionKind::Linear => {
            let mut a = DimensionLinear::new(p3(d.p1), p3(d.p2));
            a.definition_point = p3(d.vertex);
            let dx = (d.p2.x - d.p1.x).abs();
            let dy = (d.p2.y - d.p1.y).abs();
            // Prefer horizontal when wider, else vertical (AutoCAD linear axes).
            a.rotation = if dx >= dy {
                0.0
            } else {
                std::f64::consts::FRAC_PI_2
            };
            if d.measurement.abs() > f64::EPSILON {
                a.base.actual_measurement = d.measurement;
            }
            AcadDim::Linear(a)
        }
        DimensionKind::Radius => {
            let mut a = DimensionRadius::new(p3(d.vertex), p3(d.p1));
            if d.measurement.abs() > f64::EPSILON {
                a.base.actual_measurement = d.measurement;
            }
            AcadDim::Radius(a)
        }
        DimensionKind::Diameter => {
            let mut a = DimensionDiameter::new(p3(d.vertex), p3(d.p1));
            a.definition_point = p3(d.p2);
            if d.measurement.abs() > f64::EPSILON {
                a.base.actual_measurement = d.measurement;
            }
            AcadDim::Diameter(a)
        }
        DimensionKind::Angular => {
            let mut a = DimensionAngular3Pt::new(p3(d.vertex), p3(d.p1), p3(d.p2));
            a.definition_point = p3(d.text_anchor);
            if d.measurement.abs() > f64::EPSILON {
                a.base.actual_measurement = d.measurement;
            }
            AcadDim::Angular3Pt(a)
        }
        DimensionKind::OrdinateX => {
            let mut a = DimensionOrdinate::new(p3(d.p1), p3(d.text_anchor), true);
            if d.measurement.abs() > f64::EPSILON {
                a.base.actual_measurement = d.measurement;
            }
            AcadDim::Ordinate(a)
        }
        DimensionKind::OrdinateY => {
            let mut a = DimensionOrdinate::new(p3(d.p1), p3(d.text_anchor), false);
            if d.measurement.abs() > f64::EPSILON {
                a.base.actual_measurement = d.measurement;
            }
            AcadDim::Ordinate(a)
        }
    };
    {
        let base = acad.base_mut();
        base.common = with_layer(EntityCommon::new(), &d.layer);
        base.text_middle_point = p3(d.text_anchor);
        if !d.text.trim().is_empty() {
            base.text = d.text.clone();
            base.user_text = Some(d.text.clone());
            base.text_user_positioned = true;
        }
    }
    Some(EntityType::Dimension(acad))
}

fn convert_entity(cad: &CadDocument, ent: &EntityType) -> Option<(String, Geometry)> {
    match ent {
        EntityType::Line(l) => Some((
            layer_name(&l.common),
            Geometry::Line(Segment::new(
                Point2::new(l.start.x, l.start.y),
                Point2::new(l.end.x, l.end.y),
            )),
        )),
        EntityType::Circle(c) => Some((
            layer_name(&c.common),
            Geometry::Circle(Circle::new(Point2::new(c.center.x, c.center.y), c.radius)),
        )),
        EntityType::Arc(a) => Some((
            layer_name(&a.common),
            Geometry::Arc(Arc::new(
                Point2::new(a.center.x, a.center.y),
                a.radius,
                a.start_angle.to_degrees(),
                a.end_angle.to_degrees(),
            )),
        )),
        EntityType::LwPolyline(p) => {
            let verts: Vec<Point2> = p
                .vertices
                .iter()
                .map(|v| Point2::new(v.location.x, v.location.y))
                .collect();
            if verts.len() < 2 {
                return None;
            }
            let bulges: Vec<f64> = p.vertices.iter().map(|v| v.bulge).collect();
            let pl = if bulges.iter().any(|b| b.abs() > 1e-12) {
                Polyline::densify_bulges(&verts, &bulges, p.is_closed)
            } else {
                Polyline::new(verts, p.is_closed)
            };
            Some((layer_name(&p.common), Geometry::Polyline(pl)))
        }
        EntityType::Polyline2D(p) => {
            let verts: Vec<Point2> = p
                .vertices
                .iter()
                .map(|v| Point2::new(v.location.x, v.location.y))
                .collect();
            if verts.len() < 2 {
                return None;
            }
            let bulges: Vec<f64> = p.vertices.iter().map(|v| v.bulge).collect();
            let pl = if bulges.iter().any(|b| b.abs() > 1e-12) {
                Polyline::densify_bulges(&verts, &bulges, p.is_closed())
            } else {
                Polyline::new(verts, p.is_closed())
            };
            Some((layer_name(&p.common), Geometry::Polyline(pl)))
        }
        EntityType::Polyline(p) => {
            let verts: Vec<Point2> = p
                .vertices
                .iter()
                .map(|v| Point2::new(v.location.x, v.location.y))
                .collect();
            if verts.len() < 2 {
                return None;
            }
            Some((
                layer_name(&p.common),
                Geometry::Polyline(Polyline::new(verts, p.flags.is_closed())),
            ))
        }
        EntityType::Text(t) => Some((
            layer_name(&t.common),
            Geometry::Text(Text::new(
                t.value.clone(),
                Point2::new(t.insertion_point.x, t.insertion_point.y),
                t.height.max(0.01),
                t.rotation.to_degrees(),
            )),
        )),
        EntityType::MText(t) => Some((
            layer_name(&t.common),
            // `Text::new` runs `sanitize_display_text` (strips `\f…;` font/size codes).
            Geometry::Text(Text::new(
                t.value.clone(),
                Point2::new(t.insertion_point.x, t.insertion_point.y),
                t.height.max(0.01),
                t.rotation.to_degrees(),
            )),
        )),
        EntityType::Insert(i) => Some((
            layer_name(&i.common),
            Geometry::Insert(
                Insert::new(
                    i.block_name.clone(),
                    Point2::new(i.insert_point.x, i.insert_point.y),
                    i.rotation.to_degrees(),
                )
                .with_scale(i.x_scale(), i.y_scale()),
            ),
        )),
        EntityType::RasterImage(img) => {
            let mut path = img.file_path.clone();
            if path.is_empty() {
                if let Some(h) = img.definition_handle {
                    if let Some(ObjectType::ImageDefinition(def)) = cad.objects.get(&h) {
                        path = def.file_name.clone();
                    }
                }
            }
            if path.is_empty() {
                return None;
            }
            let w = img.width().abs().max(1e-9);
            let h = img.height().abs().max(1e-9);
            let rot = img.u_vector.y.atan2(img.u_vector.x).to_degrees();
            let opacity = (1.0 - img.fade as f32 / 100.0).clamp(0.0, 1.0);
            Some((
                layer_name(&img.common),
                Geometry::Image(
                    ImageRef::new(
                        path,
                        Point2::new(img.insertion_point.x, img.insertion_point.y),
                        w,
                        h,
                    )
                    .with_rotation(rot)
                    .with_opacity(opacity),
                ),
            ))
        }
        EntityType::XLine(x) => {
            // Approximate infinite construction line as a long segment.
            let len = 1.0e4;
            let ox = x.base_point.x;
            let oy = x.base_point.y;
            let (dx, dy) = normalize2(x.direction.x, x.direction.y);
            Some((
                layer_name(&x.common),
                Geometry::Line(Segment::new(
                    Point2::new(ox - dx * len, oy - dy * len),
                    Point2::new(ox + dx * len, oy + dy * len),
                )),
            ))
        }
        EntityType::AttributeEntity(a) => {
            // Block ATTRIB values often carry room tags / labels in DWGs.
            if a.value.trim().is_empty() {
                return None;
            }
            Some((
                layer_name(&a.common),
                Geometry::Text(Text::new(
                    a.value.clone(),
                    Point2::new(a.insertion_point.x, a.insertion_point.y),
                    a.height.max(0.01),
                    a.rotation.to_degrees(),
                )),
            ))
        }
        _ => None,
    }
}

fn v2(p: Vector3) -> Point2 {
    Point2::new(p.x, p.y)
}

fn dim_text_override(base: &acadrust::entities::DimensionBase) -> String {
    let raw = base
        .user_text
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| base.text.clone());
    // AutoCAD "<>" means "use measured value".
    if raw.trim() == "<>" {
        String::new()
    } else {
        aadt_model::sanitize_display_text(&raw)
    }
}

/// Explode a named block's entities into the document (WCS). Used for anonymous
/// `*D…` dimension blocks whose graphics are already in world coordinates.
fn explode_named_block(
    cad: &CadDocument,
    doc: &mut Document,
    block_name: &str,
    fallback_layer: &str,
) -> bool {
    if block_name.is_empty() {
        return false;
    }
    let Some(br) = cad.block_records.iter().find(|b| b.name == block_name) else {
        return false;
    };
    let mut n = 0usize;
    for h in &br.entity_handles {
        let Some(ent) = cad.get_entity(*h) else {
            continue;
        };
        if let Some((layer, geom)) = convert_entity(cad, ent) {
            let layer = if layer.is_empty() {
                fallback_layer.to_string()
            } else {
                layer
            };
            ensure_layer(doc, &layer);
            doc.add_geometry(layer, geom);
            n += 1;
        }
    }
    n > 0
}

fn convert_dimension(d: &acadrust::entities::Dimension) -> Option<Dimension> {
    use acadrust::entities::Dimension as AcadDim;
    let base = d.base();
    let layer = layer_name(&base.common);
    let text = dim_text_override(base);
    let text_anchor = v2(base.text_middle_point);
    let measured = if base.actual_measurement.abs() > f64::EPSILON {
        base.actual_measurement
    } else {
        d.measurement()
    };

    match d {
        AcadDim::Aligned(a) => Some(Dimension {
            space: Default::default(),
            layer,
            kind: DimensionKind::Aligned,
            p1: v2(a.first_point),
            p2: v2(a.second_point),
            vertex: v2(a.first_point),
            text_anchor,
            text,
            measurement: if measured.abs() > f64::EPSILON {
                measured
            } else {
                a.measurement()
            },
        }),
        AcadDim::Linear(a) => Some(Dimension {
            space: Default::default(),
            layer,
            kind: DimensionKind::Linear,
            p1: v2(a.first_point),
            p2: v2(a.second_point),
            vertex: v2(a.definition_point),
            text_anchor,
            text,
            measurement: if measured.abs() > f64::EPSILON {
                measured
            } else {
                a.first_point.distance(&a.second_point)
            },
        }),
        AcadDim::Radius(r) => Some(Dimension {
            space: Default::default(),
            layer,
            kind: DimensionKind::Radius,
            p1: v2(r.definition_point),
            p2: v2(r.definition_point),
            vertex: v2(r.angle_vertex),
            text_anchor,
            text,
            measurement: if measured.abs() > f64::EPSILON {
                measured
            } else {
                r.measurement()
            },
        }),
        AcadDim::Diameter(dia) => {
            let a = v2(dia.definition_point);
            let b = v2(dia.angle_vertex);
            Some(Dimension {
                space: Default::default(),
                layer,
                kind: DimensionKind::Diameter,
                p1: a,
                p2: b,
                vertex: Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5),
                text_anchor,
                text,
                measurement: if measured.abs() > f64::EPSILON {
                    measured
                } else {
                    dia.measurement()
                },
            })
        }
        AcadDim::Angular2Ln(a) => Some(Dimension {
            space: Default::default(),
            layer,
            kind: DimensionKind::Angular,
            p1: v2(a.first_point),
            p2: v2(a.second_point),
            vertex: v2(a.angle_vertex),
            text_anchor,
            text,
            measurement: if measured.abs() > f64::EPSILON {
                measured
            } else {
                a.measurement_degrees()
            },
        }),
        AcadDim::Angular3Pt(a) => Some(Dimension {
            space: Default::default(),
            layer,
            kind: DimensionKind::Angular,
            p1: v2(a.first_point),
            p2: v2(a.second_point),
            vertex: v2(a.angle_vertex),
            text_anchor,
            text,
            measurement: if measured.abs() > f64::EPSILON {
                measured
            } else {
                a.measurement_degrees()
            },
        }),
        AcadDim::Ordinate(o) => Some(Dimension {
            space: Default::default(),
            layer,
            kind: if o.is_ordinate_type_x {
                DimensionKind::OrdinateX
            } else {
                DimensionKind::OrdinateY
            },
            p1: v2(o.feature_location),
            p2: v2(o.leader_endpoint),
            vertex: v2(o.definition_point),
            text_anchor,
            text,
            measurement: if measured.abs() > f64::EPSILON {
                measured
            } else {
                o.measurement()
            },
        }),
    }
}

fn convert_hatch(h: &acadrust::entities::Hatch) -> Option<Hatch> {
    let layer = layer_name(&h.common);
    let pattern = if h.is_solid || h.pattern.name.is_empty() {
        "SOLID".to_string()
    } else {
        h.pattern.name.clone()
    };
    let mut paths: Vec<Polyline> = Vec::new();
    for path in &h.paths {
        if let Some(pl) = boundary_path_to_polyline(path) {
            if pl.vertices.len() >= 3 {
                paths.push(pl);
            }
        }
    }
    // Associative hatches sometimes lack edge geometry — fall back to AABB.
    if paths.is_empty() {
        let bb = h.bounding_box();
        let (xmin, ymin, xmax, ymax) = (bb.min.x, bb.min.y, bb.max.x, bb.max.y);
        if (xmax - xmin).abs() > 1e-9 && (ymax - ymin).abs() > 1e-9 {
            paths.push(Polyline::new(
                vec![
                    Point2::new(xmin, ymin),
                    Point2::new(xmax, ymin),
                    Point2::new(xmax, ymax),
                    Point2::new(xmin, ymax),
                ],
                true,
            ));
        }
    }
    let mut iter = paths.into_iter();
    let outer = iter.next()?;
    let mut hatch = Hatch::new(layer, outer, pattern);
    for island in iter {
        hatch.push_island(island);
    }
    Some(hatch)
}

fn boundary_path_to_polyline(path: &acadrust::entities::BoundaryPath) -> Option<Polyline> {
    use acadrust::entities::BoundaryEdge;
    let mut verts = Vec::new();
    let mut bulges = Vec::new();
    for edge in &path.edges {
        match edge {
            BoundaryEdge::Polyline(pe) => {
                for v in &pe.vertices {
                    verts.push(Point2::new(v.x, v.y));
                    bulges.push(v.z);
                }
                let closed = pe.is_closed || path.edges.len() == 1;
                return Some(if bulges.iter().any(|b| b.abs() > 1e-12) {
                    Polyline::densify_bulges(&verts, &bulges, closed)
                } else {
                    Polyline::new(verts, true)
                });
            }
            BoundaryEdge::Line(le) => {
                if verts
                    .last()
                    .map(|p| (p.x - le.start.x).abs() > 1e-9 || (p.y - le.start.y).abs() > 1e-9)
                    .unwrap_or(true)
                {
                    verts.push(Point2::new(le.start.x, le.start.y));
                    bulges.push(0.0);
                }
                verts.push(Point2::new(le.end.x, le.end.y));
                bulges.push(0.0);
            }
            BoundaryEdge::CircularArc(ae) => {
                let steps = 24usize;
                let mut sa = ae.start_angle;
                let mut ea = ae.end_angle;
                if ae.counter_clockwise {
                    while ea <= sa {
                        ea += std::f64::consts::TAU;
                    }
                } else {
                    while sa <= ea {
                        sa += std::f64::consts::TAU;
                    }
                    std::mem::swap(&mut sa, &mut ea);
                }
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let ang = sa + (ea - sa) * t;
                    let p = Point2::new(
                        ae.center.x + ae.radius * ang.cos(),
                        ae.center.y + ae.radius * ang.sin(),
                    );
                    if verts.last().map(|q| !q.coincident(p)).unwrap_or(true) {
                        verts.push(p);
                        bulges.push(0.0);
                    }
                }
            }
            _ => {}
        }
    }
    if verts.len() < 3 {
        return None;
    }
    Some(Polyline::new(verts, true))
}

fn normalize2(x: f64, y: f64) -> (f64, f64) {
    let n = (x * x + y * y).sqrt();
    if n < 1e-12 {
        (1.0, 0.0)
    } else {
        (x / n, y / n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::Segment;

    #[test]
    fn converts_line_entity() {
        let mut line = AcadLine::from_coords(0.0, 0.0, 0.0, 10.0, 0.0, 0.0);
        line.common = EntityCommon::with_layer("A-WALL");
        let (layer, geom) = convert_entity(&CadDocument::new(), &EntityType::Line(line)).unwrap();
        assert_eq!(layer, "A-WALL");
        match geom {
            Geometry::Line(s) => {
                assert!((s.a.x - 0.0).abs() < 1e-9);
                assert!((s.b.x - 10.0).abs() < 1e-9);
            }
            _ => panic!("expected line"),
        }
    }

    #[test]
    fn export_writes_layer_table_records() {
        // A drawing with a distinctively named, coloured layer (a name not
        // pre-seeded by Document::new, so its colour is our value).
        let mut doc = Document::new();
        doc.add_layer(
            Layer::new("ZZ-CUSTOM")
                .with_color(1)
                .with_lineweight_mm(0.50),
        );
        doc.add_geometry(
            "ZZ-CUSTOM",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );

        // Build the CAD document directly and assert the LAYER table is populated
        // (the bug was: entities referenced a layer that had no table record, so
        // the writer emitted a null layer handle).
        let (cad, _warnings) = to_cad_document(&doc).expect("to cad");
        let al = cad
            .layers
            .get("ZZ-CUSTOM")
            .expect("ZZ-CUSTOM layer record written");
        assert!(!al.handle.is_null(), "layer must have a real handle");
        assert_eq!(al.color, Color::from_index(1));
        assert_eq!(al.line_weight, LineWeight::from_value(50));
        assert!(cad.layers.get("0").is_some(), "default 0 preserved");

        // And it survives a real file round-trip through the DWG writer/reader.
        let dir = std::env::temp_dir().join("aadt-dwg-layers");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("layers.dwg");
        export_dwg_path(&doc, &path).expect("export");
        let reader_cad = DwgReader::from_file(&path).unwrap().read().unwrap();
        assert!(
            reader_cad.layers.get("ZZ-CUSTOM").is_some(),
            "ZZ-CUSTOM present in the exported DWG's layer table"
        );

        // And through our own importer the entity keeps its layer name — before
        // the fix the null layer handle collapsed everything onto "0", which is
        // what made re-imported drawings show a single ORG-DWG_LAYER-0.
        let back = import_dwg_path(&path).expect("re-import");
        let _ = std::fs::remove_file(&path);
        assert!(
            back.entities().iter().any(|e| e.layer == "ZZ-CUSTOM"),
            "entity layer name survives the DWG round-trip (not collapsed to 0)"
        );
    }

    #[test]
    fn dwg_round_trip_line_circle_polyline() {
        let mut doc = Document::new();
        doc.add_layer(Layer::new("0"));
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "0",
            Geometry::Circle(Circle::new(Point2::new(5.0, 5.0), 2.0)),
        );
        doc.add_geometry(
            "0",
            Geometry::Polyline(Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(4.0, 3.0),
                ],
                true,
            )),
        );
        let dir = std::env::temp_dir().join("aadt_dwg_roundtrip.dwg");
        export_dwg_path(&doc, &dir).expect("export dwg");
        let back = import_dwg_path(&dir).expect("import dwg");
        let _ = std::fs::remove_file(&dir);
        assert_eq!(back.len(), 3, "expected 3 entities, got {}", back.len());
        let kinds: Vec<_> = back
            .entities()
            .iter()
            .map(|e| match &e.geometry {
                Geometry::Line(_) => "line",
                Geometry::Circle(_) => "circle",
                Geometry::Polyline(_) => "pline",
                _ => "other",
            })
            .collect();
        assert!(kinds.contains(&"line"));
        assert!(kinds.contains(&"circle"));
        assert!(kinds.contains(&"pline"));
    }

    #[test]
    fn open_path_with_spaces_round_trips() {
        let mut doc = Document::new();
        doc.add_layer(Layer::new("0"));
        doc.add_geometry(
            "0",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        );
        let dir = std::env::temp_dir().join("aadt dwg spaces");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("floor plan.dwg");
        export_dwg_path(&doc, &path).expect("export");
        let back = crate::import_path_report(&path).expect("import with spaces in path");
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.document.len(), 1);
    }

    #[test]
    fn image_exports_as_imagedef_not_footprint() {
        let dir = std::env::temp_dir().join("aadt-dwg-image");
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("chair.png");
        // Minimal 1×1 PNG (black pixel).
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE,
            0xD4, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(&png, png_bytes).unwrap();

        let mut doc = Document::new();
        doc.add_layer(Layer::new("A-FURN-PRES"));
        doc.add_geometry(
            "A-FURN-PRES",
            Geometry::Image(ImageRef::new(
                png.to_string_lossy(),
                Point2::new(1.0, 2.0),
                0.9,
                0.6,
            )),
        );

        let (cad, warnings) = to_cad_document(&doc).expect("to cad");
        assert!(
            !warnings.iter().any(|w| w.contains("footprint")),
            "must not degrade to footprint: {warnings:?}"
        );
        let n_img = cad
            .entities()
            .filter(|e| matches!(e, EntityType::RasterImage(_)))
            .count();
        assert_eq!(n_img, 1, "expected one RasterImage entity");
        let n_def = cad
            .objects
            .values()
            .filter(|o| matches!(o, ObjectType::ImageDefinition(_)))
            .count();
        assert_eq!(n_def, 1, "expected one IMAGEDEF object");

        let dwg = dir.join("with-image.dwg");
        export_dwg_path(&doc, &dwg).expect("export dwg");
        let back = import_dwg_path(&dwg).expect("re-import");
        let _ = std::fs::remove_file(&dwg);
        let img = back
            .entities()
            .iter()
            .find_map(|e| match &e.geometry {
                Geometry::Image(i) => Some(i),
                _ => None,
            })
            .expect("image survived round-trip");
        assert!((img.width - 0.9).abs() < 1e-6);
        assert!((img.height - 0.6).abs() < 1e-6);
        assert!((img.position.x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn hatch_exports_to_dwg_not_skipped() {
        let mut doc = Document::new();
        doc.add_layer(Layer::new("A-FLOR"));
        let outer = Polyline::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 6.0),
                Point2::new(0.0, 6.0),
            ],
            true,
        );
        doc.add_hatch(Hatch::new("A-FLOR", outer, "ANSI31"));

        let (cad, warnings) = to_cad_document(&doc).expect("to cad");
        assert!(
            !warnings.iter().any(|w| w.contains("hatch")),
            "hatches must not be blanket-skipped: {warnings:?}"
        );
        let n_h = cad
            .entities()
            .filter(|e| matches!(e, EntityType::Hatch(_)))
            .count();
        assert_eq!(n_h, 1, "expected one Hatch entity");

        let path = std::env::temp_dir().join("aadt_dwg_hatch.dwg");
        export_dwg_path(&doc, &path).expect("export");
        let back = import_dwg_path(&path).expect("import");
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.hatches().len(), 1);
        let h = &back.hatches()[0];
        assert_eq!(h.layer, "A-FLOR");
        assert!(
            h.pattern.eq_ignore_ascii_case("ANSI31"),
            "pattern={}",
            h.pattern
        );
        assert!(h.boundaries[0].vertices.len() >= 3);
    }

    #[test]
    fn dimension_exports_to_dwg_not_skipped() {
        let mut doc = Document::new();
        doc.add_layer(Layer::new("AN-DIMS"));
        doc.add_dimension(Dimension {
            space: Default::default(),
            layer: "AN-DIMS".into(),
            kind: DimensionKind::Aligned,
            p1: Point2::new(0.0, 0.0),
            p2: Point2::new(5.0, 0.0),
            vertex: Point2::new(2.5, 1.0),
            text_anchor: Point2::new(2.5, 1.2),
            text: String::new(),
            measurement: 5.0,
        });

        let (cad, warnings) = to_cad_document(&doc).expect("to cad");
        assert!(
            !warnings.iter().any(|w| w.contains("dimension")),
            "dimensions must not be blanket-skipped: {warnings:?}"
        );
        let n_d = cad
            .entities()
            .filter(|e| matches!(e, EntityType::Dimension(_)))
            .count();
        assert_eq!(n_d, 1, "expected one Dimension entity");

        let path = std::env::temp_dir().join("aadt_dwg_dim.dwg");
        export_dwg_path(&doc, &path).expect("export");
        let back = import_dwg_path(&path).expect("import");
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.dimensions().len(), 1);
        let d = &back.dimensions()[0];
        assert_eq!(d.kind, DimensionKind::Aligned);
        assert!((d.p1.distance(Point2::new(0.0, 0.0))).abs() < 1e-6);
        assert!((d.p2.distance(Point2::new(5.0, 0.0))).abs() < 1e-6);
    }
}
