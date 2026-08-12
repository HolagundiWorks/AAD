//! Markup model — a non-destructive annotation layer over a document or PDF.
//!
//! Markups are data, held apart from the drawing so they never alter it (spec
//! §4.5, "non-destructive markup layer"). Each carries who made it and a note,
//! so the [`MarkupList`] is filterable by author or kind — the reviewable list a
//! coordinator works from.

use aadt_geometry::Point2;

/// The kind of annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupKind {
    /// A revision cloud around a changed area.
    Cloud,
    /// A leader/callout pointing at something.
    Callout,
    /// A text note.
    Text,
    /// A highlight over an area.
    Highlight,
    /// A measurement annotation (length/area), carrying its value in `note`.
    Measurement,
    /// Freehand stroke on sheet preview / world space (L3.2).
    Freehand,
}

impl MarkupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MarkupKind::Cloud => "cloud",
            MarkupKind::Callout => "callout",
            MarkupKind::Text => "text",
            MarkupKind::Highlight => "highlight",
            MarkupKind::Measurement => "measurement",
            MarkupKind::Freehand => "freehand",
        }
    }
}

/// A single markup: what, where, by whom, and a note.
#[derive(Debug, Clone, PartialEq)]
pub struct Markup {
    pub kind: MarkupKind,
    /// Anchor point (drawing coordinates). For freehand, the first stroke point.
    pub at: Point2,
    /// Freehand polyline in world / page space; empty for point markups.
    pub path: Vec<Point2>,
    /// When set, coordinates are on this PDF page (0-based); `None` = model /
    /// sheet-preview world space (L3.3).
    pub page: Option<u32>,
    pub author: String,
    pub note: String,
}

impl Markup {
    pub fn new(
        kind: MarkupKind,
        at: Point2,
        author: impl Into<String>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            at,
            path: Vec::new(),
            page: None,
            author: author.into(),
            note: note.into(),
        }
    }

    /// Freehand stroke markup. Requires at least two points; `at` is the first.
    pub fn freehand(
        path: Vec<Point2>,
        author: impl Into<String>,
        note: impl Into<String>,
    ) -> Option<Self> {
        Self::freehand_on_page(path, None, author, note)
    }

    /// Freehand on a PDF page (`page` = 0-based index) or model space (`None`).
    pub fn freehand_on_page(
        path: Vec<Point2>,
        page: Option<u32>,
        author: impl Into<String>,
        note: impl Into<String>,
    ) -> Option<Self> {
        if path.len() < 2 {
            return None;
        }
        Some(Self {
            kind: MarkupKind::Freehand,
            at: path[0],
            path,
            page,
            author: author.into(),
            note: note.into(),
        })
    }
}

/// The non-destructive markup layer: an ordered, filterable list.
#[derive(Debug, Clone, Default)]
pub struct MarkupList {
    markups: Vec<Markup>,
}

impl MarkupList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a markup, returning its index (a stable handle for the session).
    pub fn add(&mut self, markup: Markup) -> usize {
        self.markups.push(markup);
        self.markups.len() - 1
    }

    /// Remove a markup by index, returning it if present. Later indices shift
    /// down by one, matching a plain list.
    pub fn remove(&mut self, index: usize) -> Option<Markup> {
        (index < self.markups.len()).then(|| self.markups.remove(index))
    }

    pub fn all(&self) -> &[Markup] {
        &self.markups
    }

    pub fn len(&self) -> usize {
        self.markups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.markups.is_empty()
    }

    /// Markups of a given kind.
    pub fn by_kind(&self, kind: MarkupKind) -> Vec<&Markup> {
        self.markups.iter().filter(|m| m.kind == kind).collect()
    }

    /// Markups by a given author.
    pub fn by_author(&self, author: &str) -> Vec<&Markup> {
        self.markups.iter().filter(|m| m.author == author).collect()
    }

    /// A plain-text markup list for export/transmittal, one line each,
    /// deterministic in list order.
    pub fn to_report(&self) -> String {
        let mut out = String::new();
        for (i, m) in self.markups.iter().enumerate() {
            if m.kind == MarkupKind::Freehand {
                let page = m
                    .page
                    .map(|p| format!(" p{}", p + 1))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "{}. [{}] ({:.3}, {:.3}){} {} pts {} — {}\n",
                    i + 1,
                    m.kind.as_str(),
                    m.at.x,
                    m.at.y,
                    page,
                    m.path.len(),
                    m.author,
                    m.note
                ));
            } else {
                out.push_str(&format!(
                    "{}. [{}] ({:.3}, {:.3}) {} — {}\n",
                    i + 1,
                    m.kind.as_str(),
                    m.at.x,
                    m.at.y,
                    m.author,
                    m.note
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> MarkupList {
        let mut l = MarkupList::new();
        l.add(Markup::new(
            MarkupKind::Cloud,
            Point2::new(1.0, 2.0),
            "VH",
            "revise header",
        ));
        l.add(Markup::new(
            MarkupKind::Text,
            Point2::new(3.0, 4.0),
            "JS",
            "check dim",
        ));
        l.add(Markup::new(
            MarkupKind::Cloud,
            Point2::new(5.0, 6.0),
            "VH",
            "add note",
        ));
        l
    }

    #[test]
    fn filters_by_kind_and_author() {
        let l = list();
        assert_eq!(l.len(), 3);
        assert_eq!(l.by_kind(MarkupKind::Cloud).len(), 2);
        assert_eq!(l.by_author("VH").len(), 2);
        assert_eq!(l.by_author("JS").len(), 1);
    }

    #[test]
    fn remove_returns_the_markup_and_shifts_indices() {
        let mut l = list();
        let removed = l.remove(0).unwrap();
        assert_eq!(removed.author, "VH");
        assert_eq!(l.len(), 2);
        assert!(l.remove(9).is_none(), "out of range is None");
    }

    #[test]
    fn report_lists_every_markup_in_order() {
        let report = list().to_report();
        let lines: Vec<_> = report.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("[cloud]") && lines[0].contains("revise header"));
        assert!(lines[1].contains("JS"));
    }

    #[test]
    fn freehand_requires_two_points_and_reports_count() {
        assert!(Markup::freehand(vec![Point2::new(0.0, 0.0)], "VH", "x").is_none());
        let m = Markup::freehand(
            vec![Point2::new(0.0, 0.0), Point2::new(1.0, 1.0), Point2::new(2.0, 0.5)],
            "VH",
            "scribble",
        )
        .unwrap();
        assert_eq!(m.kind, MarkupKind::Freehand);
        assert_eq!(m.at, Point2::new(0.0, 0.0));
        assert_eq!(m.path.len(), 3);
        let mut l = MarkupList::new();
        l.add(m);
        let report = l.to_report();
        assert!(report.contains("[freehand]") && report.contains("3 pts"));
    }
}
