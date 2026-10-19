//! MATCHPROP: interactive source/destination flow and type-specific hatch fields.

use aad_doc::{EntityKind, Hatch, HatchLoop, Line};
use aad_engine::{Input, Session};
use aad_geom::{PolyVertex, Vec2, Vec3};
use serde_json::json;

fn square_hatch(pattern: &str, scale: f64, angle_deg: f64) -> EntityKind {
    EntityKind::Hatch(Hatch {
        pattern: pattern.into(),
        solid: pattern.eq_ignore_ascii_case("SOLID"),
        loops: vec![HatchLoop {
            outer: true,
            vertices: vec![
                PolyVertex::new(Vec2::new(0.0, 0.0)),
                PolyVertex::new(Vec2::new(10.0, 0.0)),
                PolyVertex::new(Vec2::new(10.0, 10.0)),
                PolyVertex::new(Vec2::new(0.0, 10.0)),
            ],
        }],
        scale,
        angle: angle_deg.to_radians(),
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
    })
}

#[test]
fn matchprop_copies_hatch_pattern_scale_and_angle() {
    let mut s = Session::new();
    let src = s.add_entity(square_hatch("ANSI31", 2.0, 45.0)).unwrap();
    let dst = s.add_entity(square_hatch("SOLID", 1.0, 0.0)).unwrap();
    let r = s.execute("matchprop", &json!({ "source": src.hex(), "targets": [dst.hex()] })).unwrap();
    assert_eq!(r["changed"], 1);
    let d = s.doc().unwrap();
    let EntityKind::Hatch(h) = &d.entity(dst).unwrap().kind else {
        panic!("expected hatch");
    };
    assert_eq!(h.pattern, "ANSI31");
    assert!((h.scale - 2.0).abs() < 1e-9);
    assert!((h.angle.to_degrees() - 45.0).abs() < 1e-6);
    assert!(!h.solid);
}

#[test]
fn matchprop_interactive_prompts_for_source_then_targets() {
    let mut s = Session::new();
    let a = s.add_entity(EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(1.0, 0.0, 0.0) })).unwrap();
    let b = s.add_entity(EntityKind::Line(Line { a: Vec3::new(0.0, 1.0, 0.0), b: Vec3::new(1.0, 1.0, 0.0) })).unwrap();
    s.doc_mut()
        .unwrap()
        .modify_entity(a, |e| {
            e.common.color = aad_color::Color::Index(1);
        })
        .unwrap();
    s.start("matchprop").unwrap();
    assert!(s.prompt_text().to_ascii_lowercase().contains("source"), "{}", s.prompt_text());
    s.input(Input::Pick(vec![a])).unwrap();
    assert!(s.prompt_text().to_ascii_lowercase().contains("object"), "{}", s.prompt_text());
    s.input(Input::Pick(vec![b])).unwrap();
    s.input(Input::Enter).unwrap();
    assert!(s.running.is_none());
    let d = s.doc().unwrap();
    assert_eq!(d.entity(b).unwrap().common.color, aad_color::Color::Index(1));
}

#[test]
fn hatch_patterns_lists_library_and_defaults() {
    let mut s = Session::new();
    let r = s.execute("hatch.patterns", &json!({})).unwrap();
    let patterns = r["patterns"].as_array().unwrap();
    assert!(patterns.iter().any(|p| p["name"] == "ANSI31"));
    assert!(patterns.iter().any(|p| p["name"] == "SOLID"));
    assert!(r["defaults"]["pattern"].is_string());
    assert!(r["defaults"]["scale"].as_f64().unwrap() > 0.0);
}
