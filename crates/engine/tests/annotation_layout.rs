//! Units, plot configuration and annotation layout: nothing is scaled silently, annotation sizes
//! follow the plot scale, and layout only ever moves text, deterministically and undoably.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use serde_json::{Value, json};

fn mm_session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s
}

fn lines(s: &Session) -> Vec<(f64, f64, f64, f64)> {
    s.doc().unwrap().model.iter().filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some((l.a.x, l.a.y, l.b.x, l.b.y)) } else { None }).collect()
}

#[test]
fn drawing_units_need_an_explicit_mode_and_never_scale_silently() {
    let mut s = mm_session();
    s.execute("line", &json!({"points": [[0, 0], [1000, 0]]})).unwrap();
    let before = s.doc().unwrap().model.clone();
    let e = s.execute("drawing.units", &json!({"units": "m"})).unwrap_err().to_string();
    assert!(e.contains("mode"), "{e}");
    assert!(s.execute("drawing.units", &json!({"units": "furlong", "mode": "metadata"})).is_err());
    assert!(s.execute("drawing.units", &json!({"units": "m", "mode": "sideways"})).is_err());

    // Metadata: the setting changes, the geometry does not, and the result says so.
    let r = s.execute("drawing.units", &json!({"units": "m", "mode": "metadata"})).unwrap();
    assert!(r["warning"].as_str().unwrap().contains("no geometry was scaled"));
    assert_eq!(s.doc().unwrap().model, before);
    assert_eq!(s.doc().unwrap().header.i64("INSUNITS", 0), 6);

    // Scale: explicit, and the factor is reported. Metres to millimetres is x1000.
    let r = s.execute("drawing.units", &json!({"units": "mm", "mode": "scale"})).unwrap();
    assert_eq!(r["factor"], json!(1000.0));
    assert_eq!(lines(&s), vec![(0.0, 0.0, 1_000_000.0, 0.0)]);
    s.execute("u", &json!({})).unwrap();
    assert_eq!(lines(&s), vec![(0.0, 0.0, 1000.0, 0.0)], "undo restores the geometry");
}

#[test]
fn unknown_drawing_units_are_not_guessed() {
    let mut s = Session::new();
    s.execute("setvar", &json!({"name": "INSUNITS", "value": 0})).unwrap();
    assert!(s.execute("display.units", &json!({"units": "m"})).is_err());
    assert!(s.execute("plot.config", &json!({"scale": "1:100", "paper": "A1"})).is_err());
    assert!(s.execute("drawing.units", &json!({"units": "m", "mode": "scale"})).is_err(), "nothing to convert from");
}

#[test]
fn display_units_change_dimension_text_not_geometry() {
    let mut s = mm_session();
    s.execute("dimlinear", &json!({"p1": [0, 0], "p2": [3000, 0], "at": [0, 500]})).unwrap();
    let before = s.doc().unwrap().model.clone();
    let r = s.execute("display.units", &json!({"units": "m"})).unwrap();
    assert_eq!(r["factor"], json!(0.001));
    assert_eq!(s.doc().unwrap().model, before, "no object changed");
    let st = &s.doc().unwrap().dim_styles[0];
    assert_eq!((st.linear_factor, st.post.as_str()), (0.001, "<> m"));
    s.execute("display.units", &json!({"units": "ftin"})).unwrap();
    let st = &s.doc().unwrap().dim_styles[0];
    assert_eq!(st.linear_unit, 4, "architectural format");
    assert_eq!(s.doc().unwrap().model, before);
}

#[test]
fn annotation_sizes_follow_the_plot_scale_and_unit() {
    let mut s = mm_session();
    let r = s.execute("plot.config", &json!({"scale": "1:100", "paper": "A1"})).unwrap();
    assert_eq!(r["sizes"]["textHeight"], json!(250.0), "2.5 mm printed at 1:100 is 250 mm in a millimetre drawing");
    assert_eq!(s.doc().unwrap().header.f64("TEXTSIZE", 0.0), 250.0);
    assert_eq!(s.doc().unwrap().dim_styles[0].text_height, 250.0);
    // The same print in a metre drawing; a different scale; a custom sheet.
    s.execute("drawing.units", &json!({"units": "m", "mode": "metadata"})).unwrap();
    let r = s.execute("plot.config", &json!({"scale": 50, "paper": [900, 600], "textMm": 3.5})).unwrap();
    assert!((r["sizes"]["textHeight"].as_f64().unwrap() - 0.175).abs() < 1e-12);
    for bad in [json!({"scale": "x", "paper": "A1"}), json!({"scale": "1:100", "paper": "B7"}), json!({"scale": "1:100", "paper": [10, 10]})] {
        assert!(s.execute("plot.config", &bad).is_err(), "{bad}");
    }
}

#[test]
fn plot_validation_flags_overflow_small_text_and_conflicts() {
    let mut s = mm_session();
    s.execute("plot.config", &json!({"scale": "1:100", "paper": "A3"})).unwrap();
    // 50 m x 10 m is 500 x 100 mm at 1:100, wider than A3's printable 400 mm.
    s.execute("rectang", &json!({"p1": [0, 0], "p2": [50_000, 10_000]})).unwrap();
    s.execute("text", &json!({"at": [100, 100], "text": "TOO SMALL", "height": 100})).unwrap();
    let r = s.execute("plot.validate", &json!({})).unwrap();
    assert_eq!(r["fits"], false);
    let w: Vec<String> = r["warnings"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(w.iter().any(|m| m.contains("larger than the printable")), "{w:?}");
    assert!(w.iter().any(|m| m.contains("print smaller")), "{w:?}");
    let mut fresh = mm_session();
    assert!(fresh.execute("plot.validate", &json!({})).is_err(), "no configuration yet");
}

fn text_count(s: &Session) -> usize {
    s.doc().unwrap().model.iter().filter(|e| matches!(e.kind, EntityKind::Text(_))).count()
}

fn model_snapshot(s: &Session) -> String {
    format!("{:?}", s.doc().unwrap().model.iter().map(|e| e.kind.clone()).collect::<Vec<_>>())
}

#[test]
fn overlapping_text_is_moved_into_clear_space_deterministically_and_undoably() {
    let mut s = mm_session();
    s.execute("line", &json!({"points": [[0, 0], [5000, 0]]})).unwrap();
    s.execute("text", &json!({"at": [1000, 20], "text": "WALL TAG", "height": 250})).unwrap();
    s.execute("text", &json!({"at": [1100, 50], "text": "ROOM", "height": 250})).unwrap();
    let geometry_before = lines(&s);
    let c = s.execute("annotation.collisions", &json!({"clearance": 50})).unwrap();
    assert!(c["count"].as_u64().unwrap() >= 2, "text over the wall and text over text: {c}");
    let kinds: Vec<&str> = c["conflicts"].as_array().unwrap().iter().map(|v| v["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"text-text") && kinds.contains(&"text-geometry"), "{kinds:?}");

    let doc_before = model_snapshot(&s);
    let preview = s.execute("annotation.layout", &json!({"mode": "preview", "clearance": 50})).unwrap();
    assert_eq!(model_snapshot(&s), doc_before, "a preview changes nothing");
    assert!(!preview["moves"].as_array().unwrap().is_empty());
    assert!(preview["unresolved"].as_array().unwrap().is_empty(), "{preview}");
    let again = s.execute("annotation.layout", &json!({"mode": "preview", "clearance": 50})).unwrap();
    assert_eq!(preview, again, "identical inputs give identical layouts");

    let applied = s.execute("annotation.layout", &json!({"mode": "apply", "clearance": 50})).unwrap();
    assert_eq!(applied["applied"].as_u64(), applied["moves"].as_array().map(|m| m.len() as u64));
    assert_eq!(lines(&s), geometry_before, "geometry is never moved");
    assert_eq!(text_count(&s), 2, "no text deleted");
    let after = s.execute("annotation.collisions", &json!({"clearance": 50})).unwrap();
    assert_eq!(after["count"], 0, "{after}");
    let settled = s.execute("annotation.layout", &json!({"mode": "preview", "clearance": 50})).unwrap();
    assert!(settled["moves"].as_array().unwrap().is_empty(), "a resolved layout is stable");

    s.execute("u", &json!({})).unwrap();
    assert_eq!(model_snapshot(&s), doc_before, "one undo step restores every text");
}

#[test]
fn manually_placed_text_is_left_alone_unless_allowed_and_impossible_layouts_are_flagged() {
    let mut s = mm_session();
    s.execute("text", &json!({"at": [0, 0], "text": "A", "height": 250})).unwrap();
    s.execute("text", &json!({"at": [50, 0], "text": "B", "height": 250})).unwrap();
    let hs: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
    s.execute("annotation.lock", &json!({"handles": hs})).unwrap();
    let r = s.execute("annotation.layout", &json!({"mode": "preview", "clearance": 10})).unwrap();
    assert!(r["moves"].as_array().unwrap().is_empty());
    assert_eq!(r["skippedManual"].as_array().unwrap().len(), 2);
    let r: Value = s.execute("annotation.layout", &json!({"mode": "preview", "clearance": 10, "includeManual": true})).unwrap();
    assert!(!r["moves"].as_array().unwrap().is_empty(), "allowed to move manual text");

    // Walled in: a one-step search cannot find room, so the conflict is reported, not hidden.
    let mut t = mm_session();
    t.execute("text", &json!({"at": [0, 0], "text": "A", "height": 250})).unwrap();
    t.execute("text", &json!({"at": [50, 0], "text": "B", "height": 250})).unwrap();
    t.execute("rectang", &json!({"p1": [-200, -200], "p2": [800, 500]})).unwrap();
    let r = t.execute("annotation.layout", &json!({"mode": "preview", "clearance": 10, "maxSteps": 1})).unwrap();
    assert!(!r["unresolved"].as_array().unwrap().is_empty(), "{r}");
    assert!(t.execute("annotation.layout", &json!({"mode": "later"})).is_err());
}
