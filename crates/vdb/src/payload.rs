//! Host-owned entity payloads inside an AAD `.vdb` record (format v2).
//!
//! Independent of `aad-doc`: the engine maps [`EntityPayload`] to/from drawing entities.

use shilpidb::{Error as CodecError, Reader, Writer};

/// Current payload encoding written by export.
pub const PAYLOAD_VERSION: u8 = 2;

/// Caps for hostile or oversized payloads.
pub const MAX_LAYER_CHARS: usize = 256;
pub const MAX_TEXT_CHARS: usize = 4_096;
pub const MAX_BLOCK_CHARS: usize = 256;
pub const MAX_TYPE_CHARS: usize = 64;
pub const MAX_POLY_VERTICES: usize = 16_384;

/// Kind tag in the v2 binary payload.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindTag {
    Stub = 0,
    Line = 1,
    Circle = 2,
    Arc = 3,
    LwPolyline = 4,
    Text = 5,
    MText = 6,
    Insert = 7,
}

impl KindTag {
    fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Stub,
            1 => Self::Line,
            2 => Self::Circle,
            3 => Self::Arc,
            4 => Self::LwPolyline,
            5 => Self::Text,
            6 => Self::MText,
            7 => Self::Insert,
            _ => return None,
        })
    }
}

/// Decoded host payload (geometry + layer).
#[derive(Debug, Clone, PartialEq)]
pub struct EntityPayload {
    pub layer: String,
    pub geom: EntityGeom,
}

/// Geometry variants supported in format v2.
#[derive(Debug, Clone, PartialEq)]
pub enum EntityGeom {
    /// Unknown / unsupported kind: type name only (import skips creating geometry).
    Stub {
        type_name: String,
    },
    Line {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
    Circle {
        cx: f64,
        cy: f64,
        radius: f64,
    },
    Arc {
        cx: f64,
        cy: f64,
        radius: f64,
        start: f64,
        end: f64,
    },
    LwPolyline {
        closed: bool,
        vertices: Vec<(f64, f64)>,
    },
    Text {
        x: f64,
        y: f64,
        height: f64,
        rotation: f64,
        value: String,
    },
    MText {
        x: f64,
        y: f64,
        height: f64,
        contents: String,
    },
    Insert {
        x: f64,
        y: f64,
        rotation: f64,
        sx: f64,
        sy: f64,
        block: String,
    },
}

impl EntityGeom {
    pub fn type_name(&self) -> &str {
        match self {
            Self::Stub { type_name } => type_name.as_str(),
            Self::Line { .. } => "Line",
            Self::Circle { .. } => "Circle",
            Self::Arc { .. } => "Arc",
            Self::LwPolyline { .. } => "LwPolyline",
            Self::Text { .. } => "Text",
            Self::MText { .. } => "MText",
            Self::Insert { .. } => "Insert",
        }
    }
}

/// Encode a v2 payload.
pub fn encode(payload: &EntityPayload) -> Result<Vec<u8>, Error> {
    validate(payload)?;
    let mut w = Writer::new();
    w.u8(PAYLOAD_VERSION);
    write_str(&mut w, &payload.layer, MAX_LAYER_CHARS)?;
    match &payload.geom {
        EntityGeom::Stub { type_name } => {
            w.u8(KindTag::Stub as u8);
            write_str(&mut w, type_name, MAX_TYPE_CHARS)?;
        }
        EntityGeom::Line { x0, y0, x1, y1 } => {
            w.u8(KindTag::Line as u8);
            write_f64s(&mut w, &[*x0, *y0, *x1, *y1])?;
        }
        EntityGeom::Circle { cx, cy, radius } => {
            w.u8(KindTag::Circle as u8);
            write_f64s(&mut w, &[*cx, *cy, *radius])?;
        }
        EntityGeom::Arc { cx, cy, radius, start, end } => {
            w.u8(KindTag::Arc as u8);
            write_f64s(&mut w, &[*cx, *cy, *radius, *start, *end])?;
        }
        EntityGeom::LwPolyline { closed, vertices } => {
            w.u8(KindTag::LwPolyline as u8);
            w.bool(*closed);
            let n = u32::try_from(vertices.len()).map_err(|_| Error::TooManyVertices { count: vertices.len(), limit: MAX_POLY_VERTICES })?;
            w.u32(n);
            for (x, y) in vertices {
                write_f64s(&mut w, &[*x, *y])?;
            }
        }
        EntityGeom::Text { x, y, height, rotation, value } => {
            w.u8(KindTag::Text as u8);
            write_f64s(&mut w, &[*x, *y, *height, *rotation])?;
            write_str(&mut w, value, MAX_TEXT_CHARS)?;
        }
        EntityGeom::MText { x, y, height, contents } => {
            w.u8(KindTag::MText as u8);
            write_f64s(&mut w, &[*x, *y, *height])?;
            write_str(&mut w, contents, MAX_TEXT_CHARS)?;
        }
        EntityGeom::Insert { x, y, rotation, sx, sy, block } => {
            w.u8(KindTag::Insert as u8);
            write_f64s(&mut w, &[*x, *y, *rotation, *sx, *sy])?;
            write_str(&mut w, block, MAX_BLOCK_CHARS)?;
        }
    }
    let bytes = w.into_bytes();
    if bytes.len() > crate::MAX_PAYLOAD {
        return Err(Error::PayloadTooLarge { len: bytes.len(), limit: crate::MAX_PAYLOAD });
    }
    Ok(bytes)
}

/// Decode a payload. Accepts v2 binary, or legacy v1 `handle\ttype\tlayer` text.
pub fn decode(bytes: &[u8]) -> Result<EntityPayload, Error> {
    if bytes.is_empty() {
        return Err(Error::Format("empty payload".into()));
    }
    // v2 starts with PAYLOAD_VERSION == 2.
    if bytes[0] == PAYLOAD_VERSION {
        return decode_v2(bytes);
    }
    // Legacy v1: UTF-8 handle\ttype\tlayer
    decode_v1(bytes)
}

fn decode_v2(bytes: &[u8]) -> Result<EntityPayload, Error> {
    let mut r = Reader::new(bytes);
    let ver = r.u8().map_err(Error::Codec)?;
    if ver != PAYLOAD_VERSION {
        return Err(Error::UnsupportedPayloadVersion(ver));
    }
    let layer = read_str(&mut r, MAX_LAYER_CHARS)?;
    let tag = KindTag::from_u8(r.u8().map_err(Error::Codec)?).ok_or_else(|| Error::Format("unknown kind tag".into()))?;
    let geom = match tag {
        KindTag::Stub => EntityGeom::Stub { type_name: read_str(&mut r, MAX_TYPE_CHARS)? },
        KindTag::Line => {
            let [x0, y0, x1, y1] = read_f64s(&mut r)?;
            EntityGeom::Line { x0, y0, x1, y1 }
        }
        KindTag::Circle => {
            let [cx, cy, radius] = read_f64s(&mut r)?;
            EntityGeom::Circle { cx, cy, radius }
        }
        KindTag::Arc => {
            let [cx, cy, radius, start, end] = read_f64s(&mut r)?;
            EntityGeom::Arc { cx, cy, radius, start, end }
        }
        KindTag::LwPolyline => {
            let closed = r.bool().map_err(Error::Codec)?;
            let n = r.u32().map_err(Error::Codec)? as usize;
            if n > MAX_POLY_VERTICES {
                return Err(Error::TooManyVertices { count: n, limit: MAX_POLY_VERTICES });
            }
            let mut vertices = Vec::with_capacity(n);
            for _ in 0..n {
                let [x, y] = read_f64s(&mut r)?;
                vertices.push((x, y));
            }
            EntityGeom::LwPolyline { closed, vertices }
        }
        KindTag::Text => {
            let [x, y, height, rotation] = read_f64s(&mut r)?;
            let value = read_str(&mut r, MAX_TEXT_CHARS)?;
            EntityGeom::Text { x, y, height, rotation, value }
        }
        KindTag::MText => {
            let [x, y, height] = read_f64s(&mut r)?;
            let contents = read_str(&mut r, MAX_TEXT_CHARS)?;
            EntityGeom::MText { x, y, height, contents }
        }
        KindTag::Insert => {
            let [x, y, rotation, sx, sy] = read_f64s(&mut r)?;
            let block = read_str(&mut r, MAX_BLOCK_CHARS)?;
            EntityGeom::Insert { x, y, rotation, sx, sy, block }
        }
    };
    Ok(EntityPayload { layer, geom })
}

fn decode_v1(bytes: &[u8]) -> Result<EntityPayload, Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Format("v1 payload is not utf-8".into()))?;
    let mut parts = text.split('\t');
    let _handle = parts.next().unwrap_or("");
    let type_name = parts.next().unwrap_or("Unknown").to_string();
    let layer = parts.next().unwrap_or("0").to_string();
    if layer.chars().count() > MAX_LAYER_CHARS || type_name.chars().count() > MAX_TYPE_CHARS {
        return Err(Error::Format("v1 string too long".into()));
    }
    Ok(EntityPayload { layer, geom: EntityGeom::Stub { type_name } })
}

fn validate(payload: &EntityPayload) -> Result<(), Error> {
    if payload.layer.chars().count() > MAX_LAYER_CHARS {
        return Err(Error::StringTooLong { field: "layer", len: payload.layer.chars().count(), limit: MAX_LAYER_CHARS });
    }
    match &payload.geom {
        EntityGeom::Stub { type_name } if type_name.chars().count() > MAX_TYPE_CHARS => {
            Err(Error::StringTooLong { field: "type", len: type_name.chars().count(), limit: MAX_TYPE_CHARS })
        }
        EntityGeom::Line { x0, y0, x1, y1 } => require_finite(&[*x0, *y0, *x1, *y1]),
        EntityGeom::Circle { cx, cy, radius } => require_finite(&[*cx, *cy, *radius]),
        EntityGeom::Arc { cx, cy, radius, start, end } => require_finite(&[*cx, *cy, *radius, *start, *end]),
        EntityGeom::LwPolyline { vertices, .. } => {
            if vertices.len() > MAX_POLY_VERTICES {
                return Err(Error::TooManyVertices { count: vertices.len(), limit: MAX_POLY_VERTICES });
            }
            for (x, y) in vertices {
                require_finite(&[*x, *y])?;
            }
            Ok(())
        }
        EntityGeom::Text { x, y, height, rotation, value } => {
            require_finite(&[*x, *y, *height, *rotation])?;
            if value.chars().count() > MAX_TEXT_CHARS {
                return Err(Error::StringTooLong { field: "text", len: value.chars().count(), limit: MAX_TEXT_CHARS });
            }
            Ok(())
        }
        EntityGeom::MText { x, y, height, contents } => {
            require_finite(&[*x, *y, *height])?;
            if contents.chars().count() > MAX_TEXT_CHARS {
                return Err(Error::StringTooLong { field: "mtext", len: contents.chars().count(), limit: MAX_TEXT_CHARS });
            }
            Ok(())
        }
        EntityGeom::Insert { x, y, rotation, sx, sy, block } => {
            require_finite(&[*x, *y, *rotation, *sx, *sy])?;
            if block.chars().count() > MAX_BLOCK_CHARS {
                return Err(Error::StringTooLong { field: "block", len: block.chars().count(), limit: MAX_BLOCK_CHARS });
            }
            Ok(())
        }
        EntityGeom::Stub { .. } => Ok(()),
    }
}

fn require_finite(vals: &[f64]) -> Result<(), Error> {
    if vals.iter().all(|v| v.is_finite()) { Ok(()) } else { Err(Error::NonFinite) }
}

fn write_str(w: &mut Writer, s: &str, max_chars: usize) -> Result<(), Error> {
    if s.chars().count() > max_chars {
        return Err(Error::StringTooLong { field: "string", len: s.chars().count(), limit: max_chars });
    }
    w.str(s);
    Ok(())
}

fn read_str(r: &mut Reader<'_>, max_chars: usize) -> Result<String, Error> {
    let s = r.str().map_err(Error::Codec)?;
    if s.chars().count() > max_chars {
        return Err(Error::StringTooLong { field: "string", len: s.chars().count(), limit: max_chars });
    }
    Ok(s)
}

fn write_f64s(w: &mut Writer, vals: &[f64]) -> Result<(), Error> {
    require_finite(vals)?;
    for v in vals {
        w.f64(*v);
    }
    Ok(())
}

fn read_f64s<const N: usize>(r: &mut Reader<'_>) -> Result<[f64; N], Error> {
    let mut out = [0.0; N];
    for slot in &mut out {
        *slot = r.f64().map_err(Error::Codec)?;
    }
    require_finite(&out)?;
    Ok(out)
}

/// Payload encode/decode errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported payload version {0}")]
    UnsupportedPayloadVersion(u8),
    #[error("payload format: {0}")]
    Format(String),
    #[error("non-finite number in payload")]
    NonFinite,
    #[error("{field} length {len} exceeds limit {limit}")]
    StringTooLong { field: &'static str, len: usize, limit: usize },
    #[error("too many polyline vertices ({count}); limit is {limit}")]
    TooManyVertices { count: usize, limit: usize },
    #[error("payload {len} bytes exceeds limit {limit}")]
    PayloadTooLarge { len: usize, limit: usize },
    #[error(transparent)]
    Codec(#[from] CodecError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(p: EntityPayload) {
        let bytes = encode(&p).expect("encode");
        let back = decode(&bytes).expect("decode");
        assert_eq!(back, p);
    }

    #[test]
    fn round_trips_each_kind() {
        round_trip(EntityPayload { layer: "0".into(), geom: EntityGeom::Line { x0: 0.0, y0: 0.0, x1: 10.0, y1: 0.0 } });
        round_trip(EntityPayload { layer: "A-WALL".into(), geom: EntityGeom::Circle { cx: 1.0, cy: 2.0, radius: 3.0 } });
        round_trip(EntityPayload { layer: "0".into(), geom: EntityGeom::Arc { cx: 0.0, cy: 0.0, radius: 5.0, start: 0.0, end: 1.57 } });
        round_trip(EntityPayload {
            layer: "0".into(),
            geom: EntityGeom::LwPolyline { closed: true, vertices: vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)] },
        });
        round_trip(EntityPayload { layer: "0".into(), geom: EntityGeom::Text { x: 0.0, y: 0.0, height: 2.5, rotation: 0.0, value: "hello".into() } });
        round_trip(EntityPayload { layer: "0".into(), geom: EntityGeom::MText { x: 0.0, y: 0.0, height: 2.5, contents: "line\\Ptwo".into() } });
        round_trip(EntityPayload {
            layer: "0".into(),
            geom: EntityGeom::Insert { x: 1.0, y: 2.0, rotation: 0.5, sx: 1.0, sy: 1.0, block: "DOOR".into() },
        });
        round_trip(EntityPayload { layer: "0".into(), geom: EntityGeom::Stub { type_name: "Hatch".into() } });
    }

    #[test]
    fn rejects_non_finite() {
        let p = EntityPayload { layer: "0".into(), geom: EntityGeom::Line { x0: f64::NAN, y0: 0.0, x1: 1.0, y1: 0.0 } };
        assert!(matches!(encode(&p), Err(Error::NonFinite)));
    }

    #[test]
    fn rejects_too_many_vertices() {
        let vertices = vec![(0.0, 0.0); MAX_POLY_VERTICES + 1];
        let p = EntityPayload { layer: "0".into(), geom: EntityGeom::LwPolyline { closed: false, vertices } };
        assert!(matches!(encode(&p), Err(Error::TooManyVertices { .. })));
    }

    #[test]
    fn reads_legacy_v1_tab_payload() {
        let bytes = b"1A\tLine\tA-WALL";
        let p = decode(bytes).expect("v1");
        assert_eq!(p.layer, "A-WALL");
        assert_eq!(p.geom, EntityGeom::Stub { type_name: "Line".into() });
    }
}
