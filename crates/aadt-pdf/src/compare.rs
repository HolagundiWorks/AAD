//! Document comparison — diff two revisions and narrate the change set.
//!
//! Spec §6.4's AADT intersection: after a `COMPARE`, AADT narrates what changed
//! for a transmittal or RFI response. The diff is **deterministic** and needs no
//! AI — an AI layer can prose it up, but the change set itself is computed.
//!
//! Two independently loaded drawings have unrelated entity ids, so entities are
//! matched by a **content fingerprint** (kind + layer + rounded geometry), not by
//! id, and compared as multisets: an entity present more times in the new
//! revision than the old is *added*, fewer times is *removed*, and the overlap is
//! *unchanged*. Coordinates are rounded to a tolerance so float noise from a
//! round-trip does not read as a change.

use std::collections::HashMap;

use aadt_model::{Document, Geometry};

/// The change to a group of like entities (same kind and layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: String,
    pub layer: String,
    pub count: usize,
}

/// The result of comparing two documents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    pub added: Vec<Change>,
    pub removed: Vec<Change>,
    pub unchanged: usize,
}

impl Diff {
    /// Whether the two revisions are identical.
    pub fn is_unchanged(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    /// Total entities added / removed across all groups.
    pub fn added_count(&self) -> usize {
        self.added.iter().map(|c| c.count).sum()
    }
    pub fn removed_count(&self) -> usize {
        self.removed.iter().map(|c| c.count).sum()
    }

    /// A plain-language change summary for a transmittal. Deterministic: groups
    /// are sorted by layer then kind.
    pub fn narrate(&self) -> String {
        if self.is_unchanged() {
            return "No changes: the two revisions are identical.".to_string();
        }
        let mut out = format!(
            "{} entit{} added, {} removed, {} unchanged.\n",
            self.added_count(),
            if self.added_count() == 1 { "y" } else { "ies" },
            self.removed_count(),
            self.unchanged,
        );
        for c in &self.added {
            out.push_str(&format!(
                "  + {} {} on {}\n",
                c.count,
                plural(&c.kind, c.count),
                c.layer
            ));
        }
        for c in &self.removed {
            out.push_str(&format!(
                "  - {} {} on {}\n",
                c.count,
                plural(&c.kind, c.count),
                c.layer
            ));
        }
        out
    }
}

fn plural(kind: &str, n: usize) -> String {
    if n == 1 {
        kind.to_string()
    } else {
        format!("{kind}s")
    }
}

/// Compare `before` and `after`, producing the change set.
pub fn compare(before: &Document, after: &Document) -> Diff {
    let a = fingerprints(before);
    let b = fingerprints(after);

    let mut keys: Vec<&Fingerprint> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();

    let mut added: HashMap<(String, String), usize> = HashMap::new();
    let mut removed: HashMap<(String, String), usize> = HashMap::new();
    let mut unchanged = 0usize;

    for fp in keys {
        let before_n = *a.get(fp).unwrap_or(&0);
        let after_n = *b.get(fp).unwrap_or(&0);
        unchanged += before_n.min(after_n);
        let group = (fp.kind.to_string(), fp.layer.clone());
        match after_n.cmp(&before_n) {
            std::cmp::Ordering::Greater => *added.entry(group).or_default() += after_n - before_n,
            std::cmp::Ordering::Less => *removed.entry(group).or_default() += before_n - after_n,
            std::cmp::Ordering::Equal => {}
        }
    }

    Diff {
        added: to_sorted_changes(added),
        removed: to_sorted_changes(removed),
        unchanged,
    }
}

fn to_sorted_changes(map: HashMap<(String, String), usize>) -> Vec<Change> {
    let mut v: Vec<Change> = map
        .into_iter()
        .map(|((kind, layer), count)| Change { kind, layer, count })
        .collect();
    // Sort by layer, then kind, for a stable, readable report.
    v.sort_by(|x, y| x.layer.cmp(&y.layer).then_with(|| x.kind.cmp(&y.kind)));
    v
}

/// A content fingerprint of an entity: kind, layer, and rounded geometry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Fingerprint {
    kind: &'static str,
    layer: String,
    geo: String,
}

/// Round to 1e-4 so a DXF round-trip's float noise does not read as a change.
fn r(v: f64) -> i64 {
    (v * 10_000.0).round() as i64
}

fn fingerprints(doc: &Document) -> HashMap<Fingerprint, usize> {
    let mut map: HashMap<Fingerprint, usize> = HashMap::new();
    for e in doc.entities() {
        let (kind, geo) = match &e.geometry {
            Geometry::Line(s) => (
                "line",
                format!("{} {} {} {}", r(s.a.x), r(s.a.y), r(s.b.x), r(s.b.y)),
            ),
            Geometry::Polyline(p) => {
                let mut g = format!("{}", p.closed);
                for v in &p.vertices {
                    g.push_str(&format!(" {} {}", r(v.x), r(v.y)));
                }
                ("polyline", g)
            }
            Geometry::Circle(c) => (
                "circle",
                format!("{} {} {}", r(c.center.x), r(c.center.y), r(c.radius)),
            ),
            Geometry::Arc(a) => (
                "arc",
                format!(
                    "{} {} {} {} {}",
                    r(a.center.x),
                    r(a.center.y),
                    r(a.radius),
                    r(a.start_angle),
                    r(a.end_angle)
                ),
            ),
            Geometry::Text(t) => (
                "text",
                format!("{} {} {}", r(t.position.x), r(t.position.y), t.value),
            ),
            Geometry::Insert(i) => (
                "insert",
                format!("{} {} {}", r(i.position.x), r(i.position.y), i.name),
            ),
        };
        let fp = Fingerprint {
            kind,
            layer: e.layer.clone(),
            geo,
        };
        *map.entry(fp).or_default() += 1;
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_geometry::{Point2, Segment};

    fn line_doc(lines: &[(&str, f64, f64, f64, f64)]) -> Document {
        let mut doc = Document::new();
        for (layer, x1, y1, x2, y2) in lines {
            doc.add_geometry(
                *layer,
                Geometry::Line(Segment::new(Point2::new(*x1, *y1), Point2::new(*x2, *y2))),
            );
        }
        doc
    }

    #[test]
    fn identical_documents_have_no_changes() {
        let a = line_doc(&[("A-WALL", 0.0, 0.0, 10.0, 0.0)]);
        let b = line_doc(&[("A-WALL", 0.0, 0.0, 10.0, 0.0)]);
        let diff = compare(&a, &b);
        assert!(diff.is_unchanged());
        assert_eq!(diff.unchanged, 1);
        assert_eq!(
            diff.narrate(),
            "No changes: the two revisions are identical."
        );
    }

    #[test]
    fn detects_added_and_removed_entities() {
        let before = line_doc(&[
            ("A-WALL", 0.0, 0.0, 10.0, 0.0),
            ("A-WALL", 0.0, 0.0, 0.0, 8.0),
        ]);
        let after = line_doc(&[
            ("A-WALL", 0.0, 0.0, 10.0, 0.0),  // kept
            ("A-WALL", 10.0, 0.0, 10.0, 8.0), // added
            ("A-GRID", 1.0, 1.0, 2.0, 2.0),   // added, different layer
        ]);
        let diff = compare(&before, &after);
        assert_eq!(diff.unchanged, 1);
        assert_eq!(diff.added_count(), 2);
        assert_eq!(diff.removed_count(), 1);
        // Added groups sorted by layer: A-GRID before A-WALL.
        assert_eq!(diff.added[0].layer, "A-GRID");
        assert_eq!(diff.added[1].layer, "A-WALL");
    }

    #[test]
    fn narration_reads_as_a_transmittal_note() {
        let before = line_doc(&[("A-WALL", 0.0, 0.0, 10.0, 0.0)]);
        let after = line_doc(&[
            ("A-WALL", 0.0, 0.0, 10.0, 0.0),
            ("A-WALL", 0.0, 8.0, 10.0, 8.0),
            ("A-WALL", 0.0, 4.0, 10.0, 4.0),
        ]);
        let note = compare(&before, &after).narrate();
        assert!(note.starts_with("2 entities added, 0 removed, 1 unchanged."));
        assert!(note.contains("+ 2 lines on A-WALL"));
    }

    #[test]
    fn float_noise_within_tolerance_is_not_a_change() {
        let a = line_doc(&[("A-WALL", 0.0, 0.0, 10.0, 0.0)]);
        let b = line_doc(&[("A-WALL", 0.000_001, 0.0, 10.0, 0.0)]);
        assert!(
            compare(&a, &b).is_unchanged(),
            "sub-tolerance move is no change"
        );
    }

    #[test]
    fn comparison_is_deterministic() {
        let a = line_doc(&[("A-WALL", 0.0, 0.0, 1.0, 0.0)]);
        let b = line_doc(&[
            ("B-GRID", 0.0, 0.0, 1.0, 0.0),
            ("A-WALL", 9.0, 9.0, 8.0, 8.0),
        ]);
        assert_eq!(compare(&a, &b), compare(&a, &b));
    }
}
