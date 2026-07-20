//! The AADT bridge — LISP functions with **read access to engine outputs**.
//!
//! This is the extensibility point spec §4.4 calls out as unique to AADT versus
//! plain CAD LISP: a routine can query the document, the detected elements, and
//! the takeoff from inside a script. Access is **read-only** — the functions take
//! a shared [`Document`] and never mutate it — so a firm macro can compute over a
//! drawing without risking it. Detection and takeoff are recomputed per call from
//! the current document, so results always reflect its present state.

use std::cell::RefCell;
use std::rc::Rc;

use aadt_detect::{detect, DetectionConfig};
use aadt_model::Document;
use aadt_takeoff::TakeoffTable;

use crate::eval::{define, Env};
use crate::value::{Native, Value};

/// Register the `aadt-*` query functions into `env`, reading from `doc`.
///
/// | function | returns |
/// |---|---|
/// | `(aadt-entity-count)` | entities in the document |
/// | `(aadt-wall-count)` / `(aadt-room-count)` / `(aadt-opening-count)` | detected element counts |
/// | `(aadt-wall-length)` | total wall length |
/// | `(aadt-net-wall-area)` | wall area net of openings |
/// | `(aadt-room-area)` | total room floor area |
pub fn install_document(env: &Env, doc: Rc<RefCell<Document>>) {
    let d = doc.clone();
    define(
        env,
        "aadt-entity-count",
        Native::new("aadt-entity-count", move |_| {
            Ok(Value::Int(d.borrow().len() as i64))
        }),
    );

    // Detection-derived counts.
    for (name, f) in [
        ("aadt-wall-count", count_walls as fn(&Document) -> i64),
        ("aadt-room-count", count_rooms),
        ("aadt-opening-count", count_openings),
        ("aadt-structural-count", count_structural),
    ] {
        let d = doc.clone();
        define(
            env,
            name,
            Native::new(name, move |_| Ok(Value::Int(f(&d.borrow())))),
        );
    }

    // Takeoff-derived totals.
    for (name, f) in [
        (
            "aadt-wall-length",
            total_wall_length as fn(&Document) -> f64,
        ),
        ("aadt-net-wall-area", total_net_wall_area),
        ("aadt-room-area", total_room_area),
    ] {
        let d = doc.clone();
        define(
            env,
            name,
            Native::new(name, move |_| Ok(Value::Real(f(&d.borrow())))),
        );
    }
}

fn detection(doc: &Document) -> aadt_detect::Detection {
    detect(doc, &DetectionConfig::default())
}

fn count_walls(doc: &Document) -> i64 {
    detection(doc).walls.len() as i64
}
fn count_rooms(doc: &Document) -> i64 {
    detection(doc).merged_rooms().len() as i64
}
fn count_openings(doc: &Document) -> i64 {
    detection(doc).openings.len() as i64
}
fn count_structural(doc: &Document) -> i64 {
    detection(doc).structural.len() as i64
}

fn takeoff(doc: &Document) -> TakeoffTable {
    TakeoffTable::from_detection(&detection(doc))
}
fn total_wall_length(doc: &Document) -> f64 {
    takeoff(doc).total_wall_length()
}
fn total_net_wall_area(doc: &Document) -> f64 {
    takeoff(doc).total_wall_net_area()
}
fn total_room_area(doc: &Document) -> f64 {
    takeoff(doc).total_room_area()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Interpreter;
    use aadt_geometry::{Point2, Polyline, Segment};
    use aadt_model::Geometry;

    fn sample_doc() -> Document {
        let mut doc = Document::new();
        // One 10-unit wall (two faces 0.2 apart).
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))),
        );
        doc.add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 0.2), Point2::new(10.0, 0.2))),
        );
        // A 4x3 room.
        doc.add_geometry(
            "A-ROOM",
            Geometry::Polyline(Polyline::new(
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

    fn interp_with(doc: Document) -> Interpreter {
        let lisp = Interpreter::new();
        install_document(lisp.env(), Rc::new(RefCell::new(doc)));
        lisp
    }

    #[test]
    fn a_script_can_query_engine_outputs() {
        let mut lisp = interp_with(sample_doc());
        assert_eq!(
            lisp.eval_str("(aadt-entity-count)").unwrap().to_string(),
            "3"
        );
        assert_eq!(lisp.eval_str("(aadt-wall-count)").unwrap().to_string(), "1");
        assert_eq!(lisp.eval_str("(aadt-room-count)").unwrap().to_string(), "1");
        assert_eq!(
            lisp.eval_str("(aadt-wall-length)").unwrap().to_string(),
            "10"
        );
        assert_eq!(lisp.eval_str("(aadt-room-area)").unwrap().to_string(), "12");
    }

    #[test]
    fn engine_queries_compose_with_lisp() {
        // A real macro: total wall length plus room area.
        let mut lisp = interp_with(sample_doc());
        let out = lisp
            .eval_str("(+ (aadt-wall-length) (aadt-room-area))")
            .unwrap();
        assert_eq!(out.to_string(), "22");
    }

    #[test]
    fn queries_reflect_the_documents_current_state() {
        let doc = Rc::new(RefCell::new(sample_doc()));
        let mut lisp = Interpreter::new();
        install_document(lisp.env(), doc.clone());
        assert_eq!(
            lisp.eval_str("(aadt-entity-count)").unwrap().to_string(),
            "3"
        );
        // Mutate the shared document; the next query sees it.
        doc.borrow_mut().add_geometry(
            "A-WALL",
            Geometry::Line(Segment::new(Point2::new(0.0, 5.0), Point2::new(1.0, 5.0))),
        );
        assert_eq!(
            lisp.eval_str("(aadt-entity-count)").unwrap().to_string(),
            "4"
        );
    }
}
