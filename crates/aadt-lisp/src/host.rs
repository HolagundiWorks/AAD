//! The AADT bridge — LISP functions over the drawing and engine outputs.
//!
//! Spec §4.4's extensibility point, unique to AADT versus plain CAD LISP: a
//! routine can **query** the document, the detected elements, and the takeoff,
//! and **create/modify** entities — all from inside a script. Both halves work
//! over one shared [`Document`], so a macro can draw, then measure what it drew.
//! Query functions recompute detection/takeoff per call, so results always
//! reflect the document's present state.

use std::cell::RefCell;
use std::rc::Rc;

use aadt_detect::{detect, DetectionConfig};
use aadt_geometry::{Arc, Circle, Point2, Polyline, Segment};
use aadt_model::{Document, EntityId, Geometry, Text};
use aadt_takeoff::TakeoffTable;

use crate::eval::{define, Env};
use crate::value::{LispError, Native, Value};

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

    install_edits(env, doc);
}

/// Register the entity create/modify functions. Each returns the new entity's
/// id as an integer (`aadt-erase`/`aadt-move` return `t`/`nil`), so a script can
/// hold and later edit what it drew.
///
/// | function | draws |
/// |---|---|
/// | `(aadt-add-line layer x1 y1 x2 y2)` | a line |
/// | `(aadt-add-circle layer cx cy r)` | a circle |
/// | `(aadt-add-arc layer cx cy r start end)` | an arc (degrees CCW) |
/// | `(aadt-add-text layer x y height "str")` | single-line text |
/// | `(aadt-add-polyline layer closed x1 y1 x2 y2 …)` | a polyline |
/// | `(aadt-move id dx dy)` | translate an entity, keeping its id |
/// | `(aadt-rotate id cx cy deg)` | rotate an entity about a pivot (returns `t`/`nil`) |
/// | `(aadt-scale id cx cy factor)` | scale an entity about a pivot |
/// | `(aadt-mirror id x1 y1 x2 y2)` | reflect an entity across an axis |
/// | `(aadt-explode id)` | bake a block reference into geometry (returns the count, or `nil`) |
/// | `(aadt-erase id)` | remove an entity |
fn install_edits(env: &Env, doc: Rc<RefCell<Document>>) {
    let d = doc.clone();
    define(
        env,
        "aadt-add-line",
        Native::new("aadt-add-line", move |a| {
            want(a, 5, "aadt-add-line")?;
            let geom = Geometry::Line(Segment::new(pt(a, 1)?, Point2::new(f(a, 3)?, f(a, 4)?)));
            Ok(added(&d, layer(a, 0)?, geom))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-add-circle",
        Native::new("aadt-add-circle", move |a| {
            want(a, 4, "aadt-add-circle")?;
            let geom = Geometry::Circle(Circle::new(pt(a, 1)?, f(a, 3)?));
            Ok(added(&d, layer(a, 0)?, geom))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-add-arc",
        Native::new("aadt-add-arc", move |a| {
            want(a, 6, "aadt-add-arc")?;
            let geom = Geometry::Arc(Arc::new(pt(a, 1)?, f(a, 3)?, f(a, 4)?, f(a, 5)?));
            Ok(added(&d, layer(a, 0)?, geom))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-add-text",
        Native::new("aadt-add-text", move |a| {
            want(a, 5, "aadt-add-text")?;
            let geom = Geometry::Text(Text::new(string(a, 4)?, pt(a, 1)?, f(a, 3)?, 0.0));
            Ok(added(&d, layer(a, 0)?, geom))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-add-polyline",
        Native::new("aadt-add-polyline", move |a| {
            // layer, closed, then x/y pairs.
            if a.len() < 6 || !(a.len() - 2).is_multiple_of(2) {
                return Err(LispError::BadArgs(
                    "aadt-add-polyline: layer, closed, then x/y pairs (>= 2 points)".into(),
                ));
            }
            let closed = a[1].is_truthy();
            let mut verts = Vec::new();
            let mut i = 2;
            while i + 1 < a.len() {
                verts.push(Point2::new(f(a, i)?, f(a, i + 1)?));
                i += 2;
            }
            let geom = Geometry::Polyline(Polyline::new(verts, closed));
            Ok(added(&d, layer(a, 0)?, geom))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-move",
        Native::new("aadt-move", move |a| {
            want(a, 3, "aadt-move")?;
            let (target, dx, dy) = (id(a, 0)?, f(a, 1)?, f(a, 2)?);
            let ok = d
                .borrow_mut()
                .transact("MOVE", |doc| doc.translate_entity(target, dx, dy));
            Ok(boolean(ok))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-rotate",
        Native::new("aadt-rotate", move |a| {
            want(a, 4, "aadt-rotate")?;
            let (target, pivot, deg) = (id(a, 0)?, pt(a, 1)?, f(a, 3)?);
            let ok = d.borrow_mut().transact("ROTATE", |doc| {
                doc.rotate_entity(target, pivot, deg.to_radians())
            });
            Ok(boolean(ok))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-scale",
        Native::new("aadt-scale", move |a| {
            want(a, 4, "aadt-scale")?;
            let (target, pivot, factor) = (id(a, 0)?, pt(a, 1)?, f(a, 3)?);
            if factor == 0.0 {
                return Err(LispError::BadArgs(
                    "aadt-scale: factor must be non-zero".into(),
                ));
            }
            let ok = d
                .borrow_mut()
                .transact("SCALE", |doc| doc.scale_entity(target, pivot, factor));
            Ok(boolean(ok))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-mirror",
        Native::new("aadt-mirror", move |a| {
            want(a, 5, "aadt-mirror")?;
            let (target, p1, p2) = (id(a, 0)?, pt(a, 1)?, pt(a, 3)?);
            if p1.coincident(p2) {
                return Err(LispError::BadArgs(
                    "aadt-mirror: the two axis points must differ".into(),
                ));
            }
            let ok = d
                .borrow_mut()
                .transact("MIRROR", |doc| doc.mirror_entity(target, p1, p2));
            Ok(boolean(ok))
        }),
    );

    let d = doc.clone();
    define(
        env,
        "aadt-explode",
        Native::new("aadt-explode", move |a| {
            want(a, 1, "aadt-explode")?;
            let target = id(a, 0)?;
            // Read the block placement first (one borrow), then mutate — so the
            // instantiation and the edit never borrow the document at once.
            let placed: Option<Vec<(String, Geometry)>> = {
                let doc = d.borrow();
                match doc.get(target).map(|e| (e.layer.clone(), &e.geometry)) {
                    Some((layer, Geometry::Insert(ins))) => doc.block(&ins.name).map(|block| {
                        block
                            .entities
                            .iter()
                            .map(|be| {
                                let l = if be.layer == "0" {
                                    layer.clone()
                                } else {
                                    be.layer.clone()
                                };
                                (
                                    l,
                                    be.geometry
                                        .instanced(block.base, ins.position, ins.rotation),
                                )
                            })
                            .collect()
                    }),
                    _ => None,
                }
            };
            match placed {
                Some(placed) => {
                    let count = placed.len() as i64;
                    d.borrow_mut().transact("EXPLODE", |doc| {
                        doc.remove_entity(target);
                        for (layer, g) in placed {
                            doc.add_geometry(layer, g);
                        }
                    });
                    Ok(Value::Int(count))
                }
                None => Ok(Value::Nil),
            }
        }),
    );

    let d = doc;
    define(
        env,
        "aadt-erase",
        Native::new("aadt-erase", move |a| {
            want(a, 1, "aadt-erase")?;
            let target = id(a, 0)?;
            let removed = d
                .borrow_mut()
                .transact("ERASE", |doc| doc.remove_entity(target));
            Ok(boolean(removed.is_some()))
        }),
    );
}

// --- Argument helpers ------------------------------------------------------

fn want(a: &[Value], n: usize, who: &str) -> Result<(), LispError> {
    if a.len() == n {
        Ok(())
    } else {
        Err(LispError::BadArgs(format!(
            "{who} expects {n} argument(s), got {}",
            a.len()
        )))
    }
}

/// A numeric argument as `f64`.
fn f(a: &[Value], i: usize) -> Result<f64, LispError> {
    a[i].as_f64()
        .ok_or_else(|| LispError::BadArgs(format!("argument {} must be a number", i + 1)))
}

/// Two consecutive numeric arguments as a point.
fn pt(a: &[Value], i: usize) -> Result<Point2, LispError> {
    Ok(Point2::new(f(a, i)?, f(a, i + 1)?))
}

/// A string argument.
fn string(a: &[Value], i: usize) -> Result<String, LispError> {
    match &a[i] {
        Value::Str(s) => Ok(s.clone()),
        other => Err(LispError::BadArgs(format!(
            "argument {} must be a string, got {other}",
            i + 1
        ))),
    }
}

/// A layer name argument (string or symbol).
fn layer(a: &[Value], i: usize) -> Result<String, LispError> {
    match &a[i] {
        Value::Str(s) => Ok(s.clone()),
        Value::Symbol(s) => Ok(s.clone()),
        other => Err(LispError::BadArgs(format!(
            "layer must be a string, got {other}"
        ))),
    }
}

/// An entity-id argument (an integer).
fn id(a: &[Value], i: usize) -> Result<EntityId, LispError> {
    match a[i] {
        Value::Int(n) if n >= 0 => Ok(EntityId(n as u64)),
        _ => Err(LispError::BadArgs(
            "entity id must be a non-negative integer".into(),
        )),
    }
}

/// Add geometry to the shared document and return its id as an integer value.
fn added(doc: &Rc<RefCell<Document>>, layer: String, geom: Geometry) -> Value {
    let id = doc
        .borrow_mut()
        .transact("LISP", |d| d.add_geometry(&layer, geom));
    Value::Int(id.0 as i64)
}

fn boolean(b: bool) -> Value {
    if b {
        Value::T
    } else {
        Value::Nil
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

    /// The shared document, so a test can inspect what a script drew.
    fn interp_and_doc() -> (Interpreter, Rc<RefCell<Document>>) {
        let doc = Rc::new(RefCell::new(Document::new()));
        let lisp = Interpreter::new();
        install_document(lisp.env(), doc.clone());
        (lisp, doc)
    }

    #[test]
    fn a_script_can_draw_entities() {
        let (mut lisp, doc) = interp_and_doc();
        let id = lisp
            .eval_str("(aadt-add-line \"A-WALL\" 0 0 10 0)")
            .unwrap();
        assert_eq!(id.to_string(), "0", "returns the new entity id");
        lisp.eval_str("(aadt-add-circle \"S-COLS\" 5 5 0.5)")
            .unwrap();
        lisp.eval_str("(aadt-add-text \"A-ANNO\" 1 1 2.5 \"NOTE\")")
            .unwrap();

        let d = doc.borrow();
        assert_eq!(d.len(), 3);
        assert!(d.entities().iter().any(|e| matches!(
            &e.geometry,
            Geometry::Text(t) if t.value == "NOTE"
        )));
    }

    #[test]
    fn a_loop_draws_a_row_of_columns() {
        // The kind of drafting macro entity-creation is for.
        let (mut lisp, doc) = interp_and_doc();
        lisp.eval_str(
            "(setq x 0)
             (repeat 5
               (aadt-add-circle \"S-COLS\" x 0 0.3)
               (setq x (+ x 6)))",
        )
        .unwrap();
        assert_eq!(doc.borrow().len(), 5, "five columns drawn");
    }

    #[test]
    fn drawn_geometry_feeds_the_engine_queries() {
        // Draw two wall faces + a room, then measure them — the read/write loop.
        let (mut lisp, _doc) = interp_and_doc();
        lisp.eval_str(
            "(aadt-add-line \"A-WALL\" 0 0 10 0)
             (aadt-add-line \"A-WALL\" 0 0.2 10 0.2)",
        )
        .unwrap();
        assert_eq!(lisp.eval_str("(aadt-wall-count)").unwrap().to_string(), "1");
        assert_eq!(
            lisp.eval_str("(aadt-wall-length)").unwrap().to_string(),
            "10"
        );
    }

    #[test]
    fn move_keeps_the_id_and_erase_removes() {
        let (mut lisp, doc) = interp_and_doc();
        let id = lisp.eval_str("(aadt-add-line \"0\" 0 0 1 0)").unwrap();
        assert_eq!(id.to_string(), "0");

        // Move by its id, then confirm the entity still exists under that id.
        assert_eq!(lisp.eval_str("(aadt-move 0 5 5)").unwrap().to_string(), "t");
        assert!(doc.borrow().get(EntityId(0)).is_some(), "id preserved");

        assert_eq!(lisp.eval_str("(aadt-erase 0)").unwrap().to_string(), "t");
        assert!(doc.borrow().get(EntityId(0)).is_none());
        // Erasing a gone entity is nil, not an error.
        assert_eq!(lisp.eval_str("(aadt-erase 0)").unwrap().to_string(), "nil");
    }

    #[test]
    fn rotate_scale_mirror_edit_in_place() {
        let (mut lisp, doc) = interp_and_doc();
        lisp.eval_str("(aadt-add-line \"0\" 0 0 2 0)").unwrap();

        // Rotate 90° about the origin: endpoint (2,0) -> (0,2).
        assert_eq!(
            lisp.eval_str("(aadt-rotate 0 0 0 90)").unwrap().to_string(),
            "t"
        );
        if let Geometry::Line(s) = &doc.borrow().get(EntityId(0)).unwrap().geometry {
            assert!(s.b.distance(Point2::new(0.0, 2.0)) < 1e-9);
        } else {
            panic!("expected line");
        }
        // Scale and mirror also report success and keep the id.
        assert_eq!(
            lisp.eval_str("(aadt-scale 0 0 0 2)").unwrap().to_string(),
            "t"
        );
        assert_eq!(
            lisp.eval_str("(aadt-mirror 0 0 0 1 0)")
                .unwrap()
                .to_string(),
            "t"
        );
        assert!(doc.borrow().get(EntityId(0)).is_some(), "id preserved");
        // A degenerate mirror axis errors rather than panicking.
        assert!(lisp.eval_str("(aadt-mirror 0 1 1 1 1)").is_err());
    }

    #[test]
    fn explode_bakes_a_block_reference() {
        use aadt_model::Block;

        let (mut lisp, doc) = interp_and_doc();
        {
            let mut b = Block::new("TEE", Point2::new(0.0, 0.0));
            b.push(
                "0",
                Geometry::Line(Segment::new(Point2::new(0.0, 0.0), Point2::new(2.0, 0.0))),
            );
            b.push(
                "STEEL",
                Geometry::Line(Segment::new(Point2::new(1.0, 0.0), Point2::new(1.0, 2.0))),
            );
            doc.borrow_mut().add_block(b);
        }
        // Place a reference to the block directly in the shared document.
        doc.borrow_mut().add_geometry(
            "A-COL",
            Geometry::Insert(aadt_model::Insert::new("TEE", Point2::new(10.0, 10.0), 0.0)),
        );
        let insert_id = doc.borrow().entities()[0].id;

        // Explode returns the number of baked entities.
        assert_eq!(
            lisp.eval_str(&format!("(aadt-explode {})", insert_id.0))
                .unwrap()
                .to_string(),
            "2"
        );
        assert!(doc.borrow().get(insert_id).is_none(), "insert removed");
        assert_eq!(doc.borrow().len(), 2);

        // Exploding a non-insert or missing id yields nil, not an error.
        assert_eq!(
            lisp.eval_str("(aadt-explode 999)").unwrap().to_string(),
            "nil"
        );
    }

    #[test]
    fn a_polyline_takes_a_variable_number_of_points() {
        let (mut lisp, doc) = interp_and_doc();
        lisp.eval_str("(aadt-add-polyline \"A-ROOM\" t 0 0 4 0 4 3 0 3)")
            .unwrap();
        let d = doc.borrow();
        match &d.entities()[0].geometry {
            Geometry::Polyline(p) => {
                assert_eq!(p.vertices.len(), 4);
                assert!(p.closed);
            }
            other => panic!("expected polyline, got {other:?}"),
        }
    }

    #[test]
    fn bad_edit_arguments_are_errors_not_panics() {
        let (mut lisp, _doc) = interp_and_doc();
        assert!(
            lisp.eval_str("(aadt-add-line \"0\" 0 0 10)").is_err(),
            "arity"
        );
        assert!(
            lisp.eval_str("(aadt-add-line 0 0 0 10 0)").is_err(),
            "layer must be a string"
        );
        assert!(
            lisp.eval_str("(aadt-add-text \"0\" 0 0 2 5)").is_err(),
            "text value must be a string"
        );
    }
}
