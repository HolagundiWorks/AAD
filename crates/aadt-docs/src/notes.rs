//! General notes — the firm notes library, placement, and rule-based suggestion.
//!
//! Notes are placed as a numbered stack of `TEXT` under a heading. The
//! **suggestion** step is deliberately deterministic: it reads the detected
//! elements and proposes the standard notes that apply (a door schedule note
//! when there are doors, a structural note when there are columns). Spec §5.3
//! calls this "AI-assisted", but the applicability rules are computable without
//! a model — an AI layer can rewrite or expand the suggested text later; it does
//! not decide *whether* a note applies. Deterministic core, AI as accelerant.

use aadt_detect::{Detection, OpeningKind, StructuralKind};
use aadt_geometry::Point2;
use aadt_model::{Document, DrawingProperties, EntityId, Floor, Geometry, Text};

/// Layer general notes are drawn on.
pub const NOTES_LAYER: &str = "A-NOTE";

/// The firm's reusable note text, grouped by the condition each note covers.
/// The library is data — a firm swaps in its own without touching the engine.
#[derive(Debug, Clone)]
pub struct NotesLibrary {
    /// Always-applicable general notes.
    pub general: Vec<String>,
    /// Note shown when the drawing contains doors/windows.
    pub openings: Vec<String>,
    /// Note shown when the drawing contains structural elements.
    pub structural: Vec<String>,
    /// Note shown when the drawing contains rooms (finish/area notes).
    pub rooms: Vec<String>,
}

impl NotesLibrary {
    /// A sensible built-in library. Firms override by constructing their own.
    pub fn standard() -> Self {
        Self {
            general: vec![
                "All dimensions are in millimetres unless noted otherwise.".into(),
                "Do not scale from this drawing. Use figured dimensions only.".into(),
                "Verify all dimensions and conditions on site before commencing work.".into(),
                "This drawing is to be read with all relevant consultants' drawings.".into(),
            ],
            openings: vec![
                "Refer to the door and window schedule for all opening sizes and types.".into(),
            ],
            structural: vec![
                "Refer to the structural engineer's drawings for all structural elements.".into(),
            ],
            rooms: vec![
                "Refer to the room finishes schedule for all floor, wall, and ceiling finishes."
                    .into(),
            ],
        }
    }

    /// The notes that apply to `det`, in a fixed order: the general notes, then
    /// openings, structural, and rooms notes when those elements are present.
    /// Deterministic — the same detection always yields the same list.
    pub fn suggest(&self, det: &Detection) -> Vec<String> {
        let mut out = self.general.clone();
        let has_openings = det
            .openings
            .iter()
            .any(|o| matches!(o.kind, OpeningKind::Door | OpeningKind::Window));
        if has_openings {
            out.extend(self.openings.iter().cloned());
        }
        let has_structural = det
            .structural
            .iter()
            .any(|s| matches!(s.kind, StructuralKind::Column | StructuralKind::Beam));
        if has_structural {
            out.extend(self.structural.iter().cloned());
        }
        if !det.rooms.is_empty() || !det.graph_rooms.is_empty() {
            out.extend(self.rooms.iter().cloned());
        }
        out
    }

    /// Like [`Self::suggest`], then appends height / floor-schedule notes from
    /// drawing properties and the document floor table.
    pub fn suggest_for_drawing(
        &self,
        det: &Detection,
        props: &DrawingProperties,
        floors: &[&Floor],
    ) -> Vec<String> {
        let mut out = self.suggest(det);
        out.extend(height_notes(props));
        out.extend(floor_schedule_notes(floors));
        out
    }
}

/// Notes derived from Drawing Properties floor / lintel height fields.
pub fn height_notes(props: &DrawingProperties) -> Vec<String> {
    let mut out = Vec::new();
    let fh = props.floor_height.trim();
    if !fh.is_empty() {
        out.push(format!("Typical floor-to-floor height is {fh}."));
    }
    let lh = props.lintel_height.trim();
    if !lh.is_empty() {
        out.push(format!(
            "Typical lintel height is {lh} above finished floor level (FFL)."
        ));
    }
    out
}

/// One note listing storey elevations when more than one floor exists.
pub fn floor_schedule_notes(floors: &[&Floor]) -> Vec<String> {
    if floors.len() < 2 {
        return Vec::new();
    }
    let parts: Vec<String> = floors
        .iter()
        .map(|f| format!("{} @ {:.3} m", f.name, f.elevation_m))
        .collect();
    vec![format!("Floor levels (FFL): {}.", parts.join("; "))]
}

impl Default for NotesLibrary {
    fn default() -> Self {
        Self::standard()
    }
}

/// Placement and sizing for a notes block.
#[derive(Debug, Clone)]
pub struct NotesStyle {
    /// Top-left corner; notes run downward from here.
    pub origin: Point2,
    pub line_height: f64,
    pub text_height: f64,
    pub heading: String,
}

impl Default for NotesStyle {
    fn default() -> Self {
        Self {
            origin: Point2::new(0.0, 0.0),
            line_height: 5.0,
            text_height: 2.5,
            heading: "GENERAL NOTES".into(),
        }
    }
}

/// Split a multiline notes body into non-empty lines (plot / properties editor).
pub fn notes_from_text(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Join note lines for the multiline editor.
pub fn notes_to_text(notes: &[String]) -> String {
    notes.join("\n")
}

/// Place a numbered notes list into `doc` under a heading, returning the ids.
/// One `UNDO` (`NOTES`) removes it. The heading is unnumbered; each note is
/// prefixed with its 1-based number.
pub fn place_notes(doc: &mut Document, notes: &[String], style: &NotesStyle) -> Vec<EntityId> {
    doc.transact("NOTES", |d| {
        let mut ids = Vec::new();
        let mut y = style.origin.y;
        let text = |d: &mut Document, s: String, x: f64, y: f64, h: f64| {
            d.add_geometry(
                NOTES_LAYER,
                Geometry::Text(Text::new(s, Point2::new(x, y), h, 0.0)),
            )
        };

        // Heading, slightly larger.
        ids.push(text(
            d,
            style.heading.clone(),
            style.origin.x,
            y,
            style.text_height * 1.3,
        ));
        y -= style.line_height * 1.6;

        for (i, note) in notes.iter().enumerate() {
            ids.push(text(
                d,
                format!("{}. {}", i + 1, note),
                style.origin.x,
                y,
                style.text_height,
            ));
            y -= style.line_height;
        }
        ids
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aadt_detect::{detect, DetectionConfig};
    use aadt_geometry::Segment;
    use aadt_model::{Document as Doc, Insert, LevelId};

    fn doc_with_door_and_room() -> Doc {
        let mut doc = Doc::new();
        // A wall (two faces) so detection has context.
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.2), Point2::new(10.0, 0.2))),
        );
        // A door insert on the wall.
        doc.add_geometry(
            "A-DOOR",
            Geometry::Insert(Insert::new("DOOR-SINGLE", Point2::new(5.0, 0.1), 0.0)),
        );
        // A room polyline.
        doc.add_geometry(
            "A-ROOM",
            Geometry::Polyline(aadt_geometry::Polyline::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(4.0, 3.0),
                    Point2::new(0.0, 3.0),
                ],
                true,
            )),
        );
        doc
    }

    #[test]
    fn suggestion_adds_notes_for_present_elements() {
        let doc = doc_with_door_and_room();
        let det = detect(&doc, &DetectionConfig::default());
        let lib = NotesLibrary::standard();
        let notes = lib.suggest(&det);

        // General notes always present.
        assert!(notes.iter().any(|n| n.contains("Do not scale")));
        // A door → opening note; a room → finishes note.
        assert!(notes.iter().any(|n| n.contains("door and window schedule")));
        assert!(notes.iter().any(|n| n.contains("room finishes schedule")));
    }

    #[test]
    fn suggestion_omits_notes_for_absent_elements() {
        // Just two bare lines — no openings, no structural, no rooms.
        let mut doc = Doc::new();
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        let det = detect(&doc, &DetectionConfig::default());
        let notes = NotesLibrary::standard().suggest(&det);
        assert!(
            notes.iter().all(|n| !n.contains("schedule")),
            "no schedule notes"
        );
        assert_eq!(notes.len(), NotesLibrary::standard().general.len());
    }

    #[test]
    fn suggestion_is_deterministic() {
        let doc = doc_with_door_and_room();
        let det = detect(&doc, &DetectionConfig::default());
        let lib = NotesLibrary::standard();
        assert_eq!(lib.suggest(&det), lib.suggest(&det));
    }

    #[test]
    fn height_notes_from_drawing_properties() {
        let props = DrawingProperties {
            floor_height: "3000 mm".into(),
            lintel_height: "2100 mm".into(),
            ..Default::default()
        };
        let notes = height_notes(&props);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("3000 mm"));
        assert!(notes[1].contains("2100 mm"));
        assert!(notes[1].contains("FFL"));
    }

    #[test]
    fn floor_schedule_note_for_multiple_floors() {
        let floors = [
            Floor {
                id: LevelId(1),
                name: "Ground".into(),
                index: 0,
                elevation_m: 0.0,
            },
            Floor {
                id: LevelId(2),
                name: "Level 1".into(),
                index: 1,
                elevation_m: 3.0,
            },
        ];
        let refs: Vec<&Floor> = floors.iter().collect();
        let notes = floor_schedule_notes(&refs);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("Ground @ 0.000 m"));
        assert!(notes[0].contains("Level 1 @ 3.000 m"));
    }

    #[test]
    fn notes_from_text_skips_blank_lines() {
        let notes = notes_from_text("one\n\n  two  \n\n");
        assert_eq!(notes, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(notes_to_text(&notes), "one\ntwo");
    }

    #[test]
    fn place_numbers_notes_and_is_undoable() {
        let mut doc = Document::new();
        let notes = vec!["First note.".to_string(), "Second note.".to_string()];
        let ids = place_notes(&mut doc, &notes, &NotesStyle::default());
        // Heading + one text per note.
        assert_eq!(ids.len(), 3);
        assert!(doc.entities().into_iter().any(|e| matches!(
            &e.geometry,
            Geometry::Text(t) if t.value == "2. Second note."
        )));
        assert_eq!(doc.undo_label(), Some("NOTES"));
        doc.undo();
        assert_eq!(doc.len(), 0);
    }
}
