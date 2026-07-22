//! Compact text encoding for [`Geometry`] inside SQLite TEXT columns.

use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
use aadt_model::{Geometry, Insert, Text};

/// Errors from native `.aadt` persistence.
#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Format(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "aadt store i/o: {e}"),
            StoreError::Sqlite(e) => write!(f, "aadt store sqlite: {e}"),
            StoreError::Format(m) => write!(f, "aadt store: {m}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
    }
}

/// Encode geometry as a single-line payload.
pub fn encode_geometry(g: &Geometry) -> Result<String, StoreError> {
    Ok(match g {
        Geometry::Line(s) => format!("LINE {} {} {} {}", s.a.x, s.a.y, s.b.x, s.b.y),
        Geometry::Circle(c) => format!("CIRCLE {} {} {}", c.center.x, c.center.y, c.radius),
        Geometry::Arc(a) => format!(
            "ARC {} {} {} {} {}",
            a.center.x, a.center.y, a.radius, a.start_angle, a.end_angle
        ),
        Geometry::Polyline(p) => {
            let mut out = format!(
                "PLINE {} {}",
                if p.closed { 1 } else { 0 },
                p.vertices.len()
            );
            for v in &p.vertices {
                out.push_str(&format!(" {} {}", v.x, v.y));
            }
            out
        }
        Geometry::Text(t) => {
            // Value is the remainder after five fields (spaces allowed).
            format!(
                "TEXT {} {} {} {} {}",
                t.position.x, t.position.y, t.height, t.rotation, t.value
            )
        }
        Geometry::Insert(i) => {
            // Block names are whitespace-free in practice; store as one token.
            format!(
                "INSERT {} {} {} {}",
                i.name, i.position.x, i.position.y, i.rotation
            )
        }
    })
}

/// Decode a payload produced by [`encode_geometry`].
pub fn decode_geometry(s: &str) -> Result<Geometry, StoreError> {
    let mut it = s.split_whitespace();
    let kind = it
        .next()
        .ok_or_else(|| StoreError::Format("empty geometry".into()))?;
    match kind {
        "LINE" => {
            let a = Point2::new(num(&mut it)?, num(&mut it)?);
            let b = Point2::new(num(&mut it)?, num(&mut it)?);
            Ok(Geometry::Line(Segment::new(a, b)))
        }
        "CIRCLE" => Ok(Geometry::Circle(Circle::new(
            Point2::new(num(&mut it)?, num(&mut it)?),
            num(&mut it)?,
        ))),
        "ARC" => Ok(Geometry::Arc(Arc::new(
            Point2::new(num(&mut it)?, num(&mut it)?),
            num(&mut it)?,
            num(&mut it)?,
            num(&mut it)?,
        ))),
        "PLINE" => {
            let closed = num(&mut it)? as i64 != 0;
            let n = num(&mut it)? as usize;
            let mut verts = Vec::with_capacity(n);
            for _ in 0..n {
                verts.push(Point2::new(num(&mut it)?, num(&mut it)?));
            }
            Ok(Geometry::Polyline(Polyline::new(verts, closed)))
        }
        "TEXT" => {
            let mut parts = s.splitn(6, char::is_whitespace);
            let _ = parts.next();
            let x: f64 = parts
                .next()
                .ok_or_else(|| StoreError::Format("TEXT missing x".into()))?
                .parse()
                .map_err(|_| StoreError::Format("TEXT bad x".into()))?;
            let y: f64 = parts
                .next()
                .ok_or_else(|| StoreError::Format("TEXT missing y".into()))?
                .parse()
                .map_err(|_| StoreError::Format("TEXT bad y".into()))?;
            let height: f64 = parts
                .next()
                .ok_or_else(|| StoreError::Format("TEXT missing height".into()))?
                .parse()
                .map_err(|_| StoreError::Format("TEXT bad height".into()))?;
            let rotation: f64 = parts
                .next()
                .ok_or_else(|| StoreError::Format("TEXT missing rotation".into()))?
                .parse()
                .map_err(|_| StoreError::Format("TEXT bad rotation".into()))?;
            let value = parts
                .next()
                .ok_or_else(|| StoreError::Format("TEXT missing value".into()))?
                .to_string();
            Ok(Geometry::Text(Text::new(
                value,
                Point2::new(x, y),
                height,
                rotation,
            )))
        }
        "INSERT" => Ok(Geometry::Insert(Insert::new(
            it.next()
                .ok_or_else(|| StoreError::Format("INSERT missing name".into()))?
                .to_string(),
            Point2::new(num(&mut it)?, num(&mut it)?),
            num(&mut it)?,
        ))),
        other => Err(StoreError::Format(format!(
            "unknown geometry kind {other:?}"
        ))),
    }
}

fn num<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<f64, StoreError> {
    let t = it
        .next()
        .ok_or_else(|| StoreError::Format("unexpected end of geometry".into()))?;
    t.parse()
        .map_err(|_| StoreError::Format(format!("not a number: {t:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_with_spaces_round_trips() {
        let g = Geometry::Text(Text::new("hello world", Point2::new(1.0, 2.0), 0.25, 15.0));
        let enc = encode_geometry(&g).unwrap();
        let dec = decode_geometry(&enc).unwrap();
        assert_eq!(g, dec);
    }

    #[test]
    fn insert_round_trips() {
        let g = Geometry::Insert(Insert::new("DOOR-01", Point2::new(3.0, 4.0), 90.0));
        let dec = decode_geometry(&encode_geometry(&g).unwrap()).unwrap();
        assert_eq!(g, dec);
    }
}
