//! Binary codec: a whole [`Document`] ⇄ the custom `.vdb` byte layout.
//!
//! Every drawing collection is written as a `u32` count followed by that many
//! fixed-layout records. Geometry is a tagged union. This mirrors the fidelity of
//! the SQLite `.aadt` store (ADR-0004) but in a format we own end to end.

use std::collections::{BTreeMap, HashMap};

use aadt_geometry::{Arc, Circle, Polyline, Segment};
use aadt_model::{
    Block, BlockParam, BlockParamKind, Dimension, DimensionKind, Document, DrawingProperties,
    Entity, EntityId, Floor, Geometry, Hatch, Insert, Layer, LevelId, LineType, ModelSpace,
    ModelSpaceId, SpaceCategory, Text, VisibilityState,
};

use crate::buf::{Reader, VdbError, Writer};

// --- Geometry ---------------------------------------------------------------

fn write_geometry(w: &mut Writer, g: &Geometry) {
    match g {
        Geometry::Line(s) => {
            w.u8(0);
            w.point(s.a);
            w.point(s.b);
        }
        Geometry::Polyline(p) => {
            w.u8(1);
            write_polyline(w, p);
        }
        Geometry::Circle(c) => {
            w.u8(2);
            w.point(c.center);
            w.f64(c.radius);
        }
        Geometry::Arc(a) => {
            w.u8(3);
            w.point(a.center);
            w.f64(a.radius);
            w.f64(a.start_angle);
            w.f64(a.end_angle);
        }
        Geometry::Text(t) => {
            w.u8(4);
            w.point(t.position);
            w.f64(t.height);
            w.f64(t.rotation);
            w.str(&t.value);
        }
        Geometry::Insert(i) => {
            w.u8(5);
            w.str(&i.name);
            w.point(i.position);
            w.f64(i.rotation);
            w.f64(i.scale_x);
            w.f64(i.scale_y);
        }
    }
}

fn read_geometry(r: &mut Reader) -> Result<Geometry, VdbError> {
    Ok(match r.u8()? {
        0 => Geometry::Line(Segment::new(r.point()?, r.point()?)),
        1 => Geometry::Polyline(read_polyline(r)?),
        2 => Geometry::Circle(Circle::new(r.point()?, r.f64()?)),
        3 => Geometry::Arc(Arc::new(r.point()?, r.f64()?, r.f64()?, r.f64()?)),
        4 => {
            let position = r.point()?;
            let height = r.f64()?;
            let rotation = r.f64()?;
            let value = r.str()?;
            Geometry::Text(Text::new(value, position, height, rotation))
        }
        5 => {
            let name = r.str()?;
            let position = r.point()?;
            let rotation = r.f64()?;
            let sx = r.f64()?;
            let sy = r.f64()?;
            Geometry::Insert(Insert::new(name, position, rotation).with_scale(sx, sy))
        }
        k => return Err(VdbError::Format(format!("unknown geometry tag {k}"))),
    })
}

fn write_polyline(w: &mut Writer, p: &Polyline) {
    w.bool(p.closed);
    w.u32(p.vertices.len() as u32);
    for v in &p.vertices {
        w.point(*v);
    }
}

fn read_polyline(r: &mut Reader) -> Result<Polyline, VdbError> {
    let closed = r.bool()?;
    let n = r.u32()? as usize;
    let mut verts = Vec::with_capacity(n);
    for _ in 0..n {
        verts.push(r.point()?);
    }
    Ok(Polyline::new(verts, closed))
}

// --- Dimension kind (7 variants) --------------------------------------------

fn dim_kind_name(k: DimensionKind) -> &'static str {
    match k {
        DimensionKind::Linear => "Linear",
        DimensionKind::Aligned => "Aligned",
        DimensionKind::Angular => "Angular",
        DimensionKind::Radius => "Radius",
        DimensionKind::Diameter => "Diameter",
        DimensionKind::OrdinateX => "OrdinateX",
        DimensionKind::OrdinateY => "OrdinateY",
    }
}

fn parse_dim_kind(s: &str) -> Result<DimensionKind, VdbError> {
    Ok(match s {
        "Linear" => DimensionKind::Linear,
        "Aligned" => DimensionKind::Aligned,
        "Angular" => DimensionKind::Angular,
        "Radius" => DimensionKind::Radius,
        "Diameter" => DimensionKind::Diameter,
        "OrdinateX" => DimensionKind::OrdinateX,
        "OrdinateY" => DimensionKind::OrdinateY,
        other => {
            return Err(VdbError::Format(format!(
                "unknown dimension kind {other:?}"
            )))
        }
    })
}

// --- Records ----------------------------------------------------------------

fn write_layer(w: &mut Writer, l: &Layer) {
    w.str(&l.name);
    w.u8(l.color);
    w.str(l.linetype.dxf_name());
    w.f64(l.lineweight_mm as f64);
    w.bool(l.visible);
    w.bool(l.frozen);
    w.bool(l.locked);
}

fn read_layer(r: &mut Reader) -> Result<Layer, VdbError> {
    let name = r.str()?;
    let color = r.u8()?;
    let lt = r.str()?;
    let lineweight_mm = r.f64()? as f32;
    let visible = r.bool()?;
    let frozen = r.bool()?;
    let locked = r.bool()?;
    Ok(Layer {
        name,
        color,
        linetype: LineType::parse(&lt).unwrap_or(LineType::Continuous),
        lineweight_mm,
        visible,
        frozen,
        locked,
    })
}

fn write_block(w: &mut Writer, b: &Block) {
    w.str(&b.name);
    w.point(b.base);
    w.u32(b.entities.len() as u32);
    for be in &b.entities {
        w.str(&be.layer);
        write_geometry(w, &be.geometry);
    }
    w.u32(b.params.len() as u32);
    for p in &b.params {
        w.str(&p.name);
        w.str(&p.default);
        w.str(p.kind.as_str());
    }
    w.u32(b.visibilities.len() as u32);
    for v in &b.visibilities {
        w.str(&v.name);
        w.u32(v.hidden.len() as u32);
        for h in &v.hidden {
            w.u64(*h as u64);
        }
    }
}

fn read_block(r: &mut Reader) -> Result<Block, VdbError> {
    let name = r.str()?;
    let base = r.point()?;
    let mut block = Block::new(name, base);
    let ne = r.u32()? as usize;
    for _ in 0..ne {
        let layer = r.str()?;
        let geom = read_geometry(r)?;
        block.push(layer, geom);
    }
    let np = r.u32()? as usize;
    block.params.clear();
    for _ in 0..np {
        let pname = r.str()?;
        let def = r.str()?;
        let kind = r.str()?;
        block.params.push(BlockParam {
            name: pname,
            default: def,
            kind: BlockParamKind::parse(&kind),
        });
    }
    let nv = r.u32()? as usize;
    let mut vis = Vec::with_capacity(nv);
    for _ in 0..nv {
        let vname = r.str()?;
        let nh = r.u32()? as usize;
        let mut hidden = Vec::with_capacity(nh);
        for _ in 0..nh {
            hidden.push(r.u64()? as usize);
        }
        vis.push(VisibilityState {
            name: vname,
            hidden,
        });
    }
    if !vis.is_empty() {
        block.visibilities = vis;
    }
    Ok(block)
}

fn write_entity(w: &mut Writer, e: &Entity) {
    w.u64(e.id.0);
    w.str(&e.layer);
    w.u64(e.space.0);
    write_geometry(w, &e.geometry);
    let mut keys: Vec<&String> = e.attributes.keys().collect();
    keys.sort();
    w.u32(keys.len() as u32);
    for k in keys {
        w.str(k);
        w.str(e.attributes.get(k).map(String::as_str).unwrap_or(""));
    }
}

fn read_entity(r: &mut Reader) -> Result<Entity, VdbError> {
    let id = r.u64()?;
    let layer = r.str()?;
    let space = r.u64()?;
    let geometry = read_geometry(r)?;
    let na = r.u32()? as usize;
    let mut attributes = HashMap::with_capacity(na);
    for _ in 0..na {
        let k = r.str()?;
        let v = r.str()?;
        attributes.insert(k, v);
    }
    Ok(Entity {
        id: EntityId(id),
        layer,
        space: ModelSpaceId(space),
        geometry,
        attributes,
    })
}

fn write_hatch(w: &mut Writer, h: &Hatch) {
    w.str(&h.layer);
    w.str(&h.pattern);
    w.bool(h.solid);
    w.u64(h.space.0);
    w.u32(h.boundaries.len() as u32);
    for poly in &h.boundaries {
        write_polyline(w, poly);
    }
}

fn read_hatch(r: &mut Reader) -> Result<Hatch, VdbError> {
    let layer = r.str()?;
    let pattern = r.str()?;
    let solid = r.bool()?;
    let space = r.u64()?;
    let nb = r.u32()? as usize;
    let mut boundaries = Vec::with_capacity(nb);
    for _ in 0..nb {
        boundaries.push(read_polyline(r)?);
    }
    if boundaries.is_empty() {
        return Err(VdbError::Format("hatch has no boundary paths".into()));
    }
    Ok(Hatch {
        layer,
        space: ModelSpaceId(space),
        boundaries,
        pattern,
        solid,
    })
}

fn write_dimension(w: &mut Writer, d: &Dimension) {
    w.str(&d.layer);
    w.str(dim_kind_name(d.kind));
    w.point(d.p1);
    w.point(d.p2);
    w.point(d.vertex);
    w.point(d.text_anchor);
    w.str(&d.text);
    w.f64(d.measurement);
    w.u64(d.space.0);
}

fn read_dimension(r: &mut Reader) -> Result<Dimension, VdbError> {
    let layer = r.str()?;
    let kind = parse_dim_kind(&r.str()?)?;
    let p1 = r.point()?;
    let p2 = r.point()?;
    let vertex = r.point()?;
    let text_anchor = r.point()?;
    let text = r.str()?;
    let measurement = r.f64()?;
    let space = r.u64()?;
    Ok(Dimension {
        space: ModelSpaceId(space),
        layer,
        kind,
        p1,
        p2,
        vertex,
        text_anchor,
        text,
        measurement,
    })
}

// --- Whole document ---------------------------------------------------------

/// Encode the whole document body (everything after the file header).
pub fn encode_document(w: &mut Writer, doc: &Document) {
    // Layers.
    let layers: Vec<&Layer> = doc.layers().collect();
    w.u32(layers.len() as u32);
    for l in layers {
        write_layer(w, l);
    }

    // Blocks.
    let blocks = doc.blocks();
    w.u32(blocks.len() as u32);
    for b in &blocks {
        write_block(w, b);
    }

    // Entities (ascending id → deterministic bytes).
    let mut ents: Vec<&Entity> = doc.entities();
    ents.sort_by_key(|e| e.id.0);
    w.u32(ents.len() as u32);
    for e in ents {
        write_entity(w, e);
    }

    // Hatches.
    w.u32(doc.hatches().len() as u32);
    for h in doc.hatches() {
        write_hatch(w, h);
    }

    // Dimensions.
    w.u32(doc.dimensions().len() as u32);
    for d in doc.dimensions() {
        write_dimension(w, d);
    }

    // Floors + model spaces.
    let floors: Vec<&Floor> = doc.floors();
    w.u32(floors.len() as u32);
    for f in floors {
        w.u64(f.id.0);
        w.str(&f.name);
        w.u32(f.index);
        w.f64(f.elevation_m);
    }
    let spaces: Vec<&ModelSpace> = doc.model_spaces().collect();
    w.u32(spaces.len() as u32);
    for s in spaces {
        w.u64(s.id.0);
        w.u64(s.floor.0);
        w.str(s.category.as_str());
        w.str(&s.name);
    }

    // Layer → category map.
    let map = doc.layer_space_map();
    w.u32(map.len() as u32);
    for (layer, cat) in map {
        w.str(layer);
        w.str(cat.as_str());
    }

    // Meta.
    w.point(doc.reference_origin());
    w.u64(doc.active_space().0);
    w.bool(doc.trace_below());
    w.bool(doc.trace_above());
    w.u64(doc.next_floor_id());
    w.u64(doc.next_space_id());

    // Properties.
    write_properties(w, doc.properties());
}

/// Decode a document body produced by [`encode_document`].
pub fn decode_document(r: &mut Reader) -> Result<Document, VdbError> {
    let mut doc = Document::new();

    let n = r.u32()? as usize;
    for _ in 0..n {
        doc.restore_layer(read_layer(r)?);
    }

    let n = r.u32()? as usize;
    for _ in 0..n {
        doc.add_block(read_block(r)?);
    }

    let n = r.u32()? as usize;
    for _ in 0..n {
        doc.restore_entity(read_entity(r)?);
    }

    let n = r.u32()? as usize;
    for _ in 0..n {
        doc.add_hatch(read_hatch(r)?);
    }

    let n = r.u32()? as usize;
    for _ in 0..n {
        doc.add_dimension(read_dimension(r)?);
    }

    // Floors + spaces.
    let nf = r.u32()? as usize;
    let mut floors = Vec::with_capacity(nf);
    for _ in 0..nf {
        let id = LevelId(r.u64()?);
        let name = r.str()?;
        let index = r.u32()?;
        let elevation_m = r.f64()?;
        floors.push(Floor {
            id,
            name,
            index,
            elevation_m,
        });
    }
    let ns = r.u32()? as usize;
    let mut spaces = Vec::with_capacity(ns);
    for _ in 0..ns {
        let id = ModelSpaceId(r.u64()?);
        let floor = LevelId(r.u64()?);
        let cat = r.str()?;
        let name = r.str()?;
        spaces.push(ModelSpace {
            id,
            floor,
            category: SpaceCategory::parse(&cat).unwrap_or(SpaceCategory::Walls),
            name,
        });
    }

    // Layer → category map.
    let nm = r.u32()? as usize;
    let mut map: BTreeMap<String, SpaceCategory> = BTreeMap::new();
    for _ in 0..nm {
        let layer = r.str()?;
        let cat = r.str()?;
        if let Some(c) = SpaceCategory::parse(&cat) {
            map.insert(layer, c);
        }
    }

    // Meta.
    let origin = r.point()?;
    let active = ModelSpaceId(r.u64()?);
    let trace_below = r.bool()?;
    let trace_above = r.bool()?;
    let next_floor = r.u64()?;
    let next_space = r.u64()?;

    if !floors.is_empty() && !spaces.is_empty() {
        doc.restore_floors(
            floors,
            spaces,
            active,
            origin,
            trace_below,
            trace_above,
            next_floor,
            next_space,
        );
    } else {
        doc.set_reference_origin(origin);
        doc.assign_imported_by_layer();
    }

    if !map.is_empty() {
        doc.restore_layer_space_map(map);
    }

    doc.set_properties(read_properties(r)?);
    Ok(doc)
}

// --- Drawing properties (fixed field order) ---------------------------------

/// Every persisted property field, in a stable order for both write and read.
macro_rules! for_each_prop {
    ($p:expr, $f:ident) => {{
        let p = $p;
        $f!(p, title);
        $f!(p, subject);
        $f!(p, author);
        $f!(p, keywords);
        $f!(p, comments);
        $f!(p, hyperlink_base);
        $f!(p, project);
        $f!(p, client);
        $f!(p, address);
        $f!(p, architect);
        $f!(p, date);
        $f!(p, area);
        $f!(p, scale);
        $f!(p, sheet_number);
        $f!(p, firm_name);
        $f!(p, firm_tagline);
        $f!(p, phone);
        $f!(p, email);
        $f!(p, office_address);
        $f!(p, logo_path);
        $f!(p, project_number);
        $f!(p, discipline);
        $f!(p, drawing_number);
        $f!(p, total_sheets);
        $f!(p, drawn_by);
        $f!(p, checked_by);
        $f!(p, approved_by);
        $f!(p, revision);
        $f!(p, drawing_status);
        $f!(p, website);
        $f!(p, revision_history);
        $f!(p, units);
        $f!(p, dimstyle);
        $f!(p, dim_text_height);
        $f!(p, dim_scale);
        $f!(p, dim_secondary);
        $f!(p, autonum_prefix);
        $f!(p, autonum_next);
        $f!(p, standard_notes);
        $f!(p, plot_include_notes);
        $f!(p, nameplate_preset);
        $f!(p, nameplate_block);
        $f!(p, takeoff_live);
        $f!(p, floor_height);
        $f!(p, lintel_height);
    }};
}

fn write_properties(w: &mut Writer, p: &DrawingProperties) {
    macro_rules! put {
        ($p:ident, $field:ident) => {
            w.str(&$p.$field);
        };
    }
    for_each_prop!(p, put);
}

fn read_properties(r: &mut Reader) -> Result<DrawingProperties, VdbError> {
    let mut p = DrawingProperties::default();
    macro_rules! get {
        ($p:ident, $field:ident) => {
            $p.$field = r.str()?;
        };
    }
    for_each_prop!(&mut p, get);
    Ok(p)
}
