//! The wire format — a dependency-free, line-oriented encoding of an [`Op`]
//! plus ephemeral presence lines (`J` / `L` / `P`).
//!
//! A transport ships operations as bytes, so each [`Op`] serialises to exactly
//! one line: tab-separated fields, with tab / newline / backslash escaped inside
//! the free-text fields so the framing is unambiguous. Floats use Rust's default
//! `f64` formatting, which is the shortest string that round-trips, so an encoded
//! op decodes back to an equal op — the property [`roundtrips`](self) rests on.
//! No JSON, no serde: the format is small enough to read on the wire.
//!
//! Presence lines are **not** stored in the relay backlog:
//! - `J\tsite` — join
//! - `L\tsite` — leave
//! - `P\tsite\tx\ty` — cursor pulse

use aadt_geometry::Point2;
use aadt_pdf::{Markup, MarkupKind};

use crate::op::{Action, Op, OpId};

/// Ephemeral presence message (not an [`Op`]).
#[derive(Debug, Clone, PartialEq)]
pub enum PresenceMsg {
    Join { site: String },
    Leave { site: String },
    Cursor { site: String, at: Point2 },
}

/// Encode one operation as a single line (no trailing newline). Add lines start
/// with `A`, remove lines with `R`.
pub fn encode_op(op: &Op) -> String {
    match &op.action {
        Action::Add(m) => format!(
            "A\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            esc(&op.id.site),
            op.id.seq,
            op.clock,
            m.kind.as_str(),
            m.at.x,
            m.at.y,
            esc(&m.author),
            esc(&m.note),
        ),
        Action::Remove(target) => format!(
            "R\t{}\t{}\t{}\t{}\t{}",
            esc(&op.id.site),
            op.id.seq,
            op.clock,
            esc(&target.site),
            target.seq,
        ),
    }
}

/// Encode a presence message as one line.
pub fn encode_presence(msg: &PresenceMsg) -> String {
    match msg {
        PresenceMsg::Join { site } => format!("J\t{}", esc(site)),
        PresenceMsg::Leave { site } => format!("L\t{}", esc(site)),
        PresenceMsg::Cursor { site, at } => {
            format!("P\t{}\t{}\t{}", esc(site), at.x, at.y)
        }
    }
}

/// Decode a line produced by [`encode_op`]. Returns `None` for a malformed line
/// (a transport skips those rather than tearing down the session).
pub fn decode_op(line: &str) -> Option<Op> {
    let f: Vec<&str> = line.split('\t').collect();
    match *f.first()? {
        "A" if f.len() == 9 => {
            let id = OpId::new(unesc(f[1]), f[2].parse().ok()?);
            let clock = f[3].parse().ok()?;
            let kind = parse_kind(f[4])?;
            let at = Point2::new(f[5].parse().ok()?, f[6].parse().ok()?);
            let markup = Markup::new(kind, at, unesc(f[7]), unesc(f[8]));
            Some(Op {
                id,
                clock,
                action: Action::Add(markup),
            })
        }
        "R" if f.len() == 6 => {
            let id = OpId::new(unesc(f[1]), f[2].parse().ok()?);
            let clock = f[3].parse().ok()?;
            let target = OpId::new(unesc(f[4]), f[5].parse().ok()?);
            Some(Op {
                id,
                clock,
                action: Action::Remove(target),
            })
        }
        _ => None,
    }
}

/// Decode a presence line (`J` / `L` / `P`). Returns `None` if not a presence line.
pub fn decode_presence(line: &str) -> Option<PresenceMsg> {
    let f: Vec<&str> = line.split('\t').collect();
    match *f.first()? {
        "J" if f.len() == 2 => Some(PresenceMsg::Join {
            site: unesc(f[1]),
        }),
        "L" if f.len() == 2 => Some(PresenceMsg::Leave {
            site: unesc(f[1]),
        }),
        "P" if f.len() == 4 => Some(PresenceMsg::Cursor {
            site: unesc(f[1]),
            at: Point2::new(f[2].parse().ok()?, f[3].parse().ok()?),
        }),
        _ => None,
    }
}

fn parse_kind(s: &str) -> Option<MarkupKind> {
    Some(match s {
        "cloud" => MarkupKind::Cloud,
        "callout" => MarkupKind::Callout,
        "text" => MarkupKind::Text,
        "highlight" => MarkupKind::Highlight,
        "measurement" => MarkupKind::Measurement,
        _ => return None,
    })
}

/// Escape the field separators so a free-text field can hold anything.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

/// Reverse [`esc`].
fn unesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(site: &str, seq: u64, clock: u64, note: &str) -> Op {
        Op {
            id: OpId::new(site, seq),
            clock,
            action: Action::Add(Markup::new(
                MarkupKind::Cloud,
                Point2::new(1.5, -2.25),
                "author",
                note,
            )),
        }
    }

    #[test]
    fn add_op_roundtrips() {
        let op = add("alice", 3, 7, "revise header");
        let line = encode_op(&op);
        assert!(!line.contains('\n'), "one line, no embedded newline");
        assert_eq!(decode_op(&line), Some(op));
    }

    #[test]
    fn remove_op_roundtrips() {
        let op = Op {
            id: OpId::new("bob", 2),
            clock: 5,
            action: Action::Remove(OpId::new("alice", 3)),
        };
        assert_eq!(decode_op(&encode_op(&op)), Some(op));
    }

    #[test]
    fn free_text_with_separators_survives() {
        // A note containing a tab, newline, and backslash must round-trip.
        let op = add("s", 1, 1, "a\tb\nc\\d");
        let line = encode_op(&op);
        assert_eq!(line.matches('\t').count(), 8, "only the 8 field separators");
        let back = decode_op(&line).unwrap();
        let Action::Add(m) = back.action else {
            panic!()
        };
        assert_eq!(m.note, "a\tb\nc\\d");
    }

    #[test]
    fn float_coordinates_round_trip_exactly() {
        let op = Op {
            id: OpId::new("s", 1),
            clock: 1,
            action: Action::Add(Markup::new(
                MarkupKind::Text,
                Point2::new(0.1 + 0.2, 1.0 / 3.0),
                "a",
                "n",
            )),
        };
        assert_eq!(decode_op(&encode_op(&op)), Some(op));
    }

    #[test]
    fn malformed_lines_are_rejected() {
        assert_eq!(decode_op(""), None);
        assert_eq!(decode_op("A\tonly\tthree"), None);
        assert_eq!(decode_op("Z\t1\t2\t3"), None);
        assert_eq!(decode_op("A\ts\tNOTNUM\t1\tcloud\t0\t0\ta\tn"), None);
    }
}
