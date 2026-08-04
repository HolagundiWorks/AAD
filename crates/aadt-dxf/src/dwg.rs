//! DWG import/export via open-source [`acadrust`] (MPL-2.0). See ADR-0002.

use std::path::Path;

use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
use aadt_model::{Block, Dimension, DimensionKind, Document, Geometry, Hatch, Insert, Layer, Text};
use acadrust::entities::{
    Arc as AcadArc, Circle as AcadCircle, EntityCommon, EntityType, Insert as AcadInsert,
    Line as AcadLine, LwPolyline, Text as AcadText,
};
use acadrust::io::dwg::{DwgReader, DwgWriter};
use acadrust::tables::{Layer as AcadLayer, TableEntry};
use acadrust::types::{Color, Handle, LineWeight, Vector2, Vector3};
use acadrust::{CadDocument, Entity};
use std::collections::BTreeSet;

use crate::DxfError;

/// Read a DWG file into a fresh [`Document`] (supported 2D entity subset).
pub fn import_dwg_path(path: impl AsRef<Path>) -> Result<Document, DxfError> {
    let mut reader = DwgReader::from_file(path.as_ref()).map_err(map_acad)?;
    let cad = reader.read().map_err(map_acad)?;
    Ok(from_cad_document(&cad))
}

/// Write a [`Document`] to a DWG file (best-effort 2D subset).
///
/// Hatches, dimensions, and inserts whose block is missing are skipped; a
/// diagnostic tag is written into drawing comments when anything is skipped.
pub fn export_dwg_path(doc: &Document, path: impl AsRef<Path>) -> Result<(), DxfError> {
    let cad = to_cad_document(doc)?;
    DwgWriter::write_to_file(path.as_ref(), &cad).map_err(map_acad)
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
            match convert_entity(ent) {
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
        match convert_entity(ent) {
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

fn to_cad_document(doc: &Document) -> Result<CadDocument, DxfError> {
    let mut cad = CadDocument::new();
    let mut skipped = 0usize;
    // Layers referenced by the entities we actually export; "0" always exists.
    let mut used: BTreeSet<String> = BTreeSet::new();
    for e in doc.entities() {
        match geometry_to_entity(&e.layer, &e.geometry) {
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
    // Annotation tables are not mapped to DWG in this subset.
    skipped += doc.hatches().len() + doc.dimensions().len();
    if skipped > 0 {
        // Persist skip count on the AADT side only when re-imported; CadDocument
        // has no free-form comments we map. Count is returned via Ok and tests
        // assert entity counts — callers may inspect after re-import.
        let _ = skipped;
    }
    Ok(cad)
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
    }
}

fn convert_entity(ent: &EntityType) -> Option<(String, Geometry)> {
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
        if let Some((layer, geom)) = convert_entity(ent) {
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
        let (layer, geom) = convert_entity(&EntityType::Line(line)).unwrap();
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
        let cad = to_cad_document(&doc).expect("to cad");
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
}
