//! Property-based tests of the editing engine.
//!
//! 1. **No command panics on any input.** `Session::execute` catches a panic and returns
//!    `EngineError::Internal`, so a panic does not crash the program but is still a bug: it is
//!    reported here as a failure. (The older `hostile_params_never_panic` test ignores results, so a
//!    caught panic passed it.)
//! 2. **A command that fails changes nothing.** The drawing after an error is identical to the one
//!    before: no half-applied edit.
//! 3. **Undo restores, redo re-applies.** After any sequence of edits, undoing them all gives the
//!    original drawing and redoing them all gives the edited one.
//!
//! Failing cases are shrunk and saved in `tests/properties.proptest-regressions` (commit that file).
//! `PROPTEST_CASES` raises the number of cases.

use aad_engine::{EngineError, Session, command_specs};
use proptest::prelude::*;
use serde_json::{Value, json};

/// A drawing with a mix of entities to edit and a few layers.
fn session() -> Session {
    let mut s = Session::new();
    s.execute("drawing.units", &json!({"units": "mm", "mode": "metadata"})).unwrap();
    s.execute("layer.new", &json!({"name": "A"})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [100, 0], [100, 50]]})).unwrap();
    s.execute("circle", &json!({"center": [30, 30], "radius": 10})).unwrap();
    s.execute("rectang", &json!({"p1": [10, 10], "p2": [60, 40]})).unwrap();
    s.execute("arc", &json!({"center": [0, 0], "radius": 25, "start": 0, "end": 90})).unwrap();
    s.execute("text", &json!({"at": [5, 5], "height": 3, "text": "Hello"})).ok();
    s
}

fn handles(s: &Session) -> Vec<String> {
    s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect()
}

/// What "the drawing" is for comparisons: every entity (handle, properties, geometry), the layer
/// names and the block names.
fn state(s: &Session) -> (Vec<aad_engine::doc::Entity>, Vec<String>, Vec<String>) {
    let d = s.doc().unwrap();
    (d.model.iter().map(|e| (**e).clone()).collect(), d.layers.iter().map(|l| l.name.clone()).collect(), d.blocks.keys().cloned().collect())
}

// ---------------- 1. hostile JSON ----------------

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        prop_oneof![Just(0.0f64), Just(-0.0), Just(1e308), Just(-1e308), Just(1e-308), Just(f64::MAX), Just(f64::MIN_POSITIVE), -1e4..1e4f64]
            .prop_map(|x| json!(x)),
        prop_oneof![Just(0i64), Just(-1), Just(i64::MAX), Just(i64::MIN), Just(4_294_967_296), -1000..1000i64].prop_map(|x| json!(x)),
        "[a-zA-Z0-9 ,.:;_|<>{}\\[\\]\"'\\\\-]{0,12}".prop_map(Value::from),
        prop_oneof![Just("zz"), Just("0"), Just("FFFFFFFF"), Just("model"), Just("A")].prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..5).prop_map(Value::Array),
            prop::collection::hash_map(key(), inner, 0..5).prop_map(|m| Value::Object(m.into_iter().collect()))
        ]
    })
}

fn key() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("points".to_string()),
        Just("handles".to_string()),
        Just("handle".to_string()),
        Just("center".to_string()),
        Just("radius".to_string()),
        Just("delta".to_string()),
        Just("base".to_string()),
        Just("angle".to_string()),
        Just("factor".to_string()),
        Just("p1".to_string()),
        Just("p2".to_string()),
        Just("at".to_string()),
        Just("name".to_string()),
        Just("text".to_string()),
        Just("height".to_string()),
        Just("count".to_string()),
        Just("distance".to_string()),
        Just("layer".to_string()),
        Just("window".to_string()),
        "[a-z]{1,8}",
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { max_shrink_iters: 200, ..ProptestConfig::default() })]

    #[test]
    fn no_command_panics_on_any_json(cmd in 0usize..400, params in json_value()) {
        let specs = command_specs();
        let spec = &specs[cmd % specs.len()];
        // Commands that touch the file system or the clipboard are not run on random paths.
        if ["open", "save", "saveas", "qsave", "recover", "wblock", "plot", "exportpdf", "close", "closeall", "new", "document.bytes", "bridge.export", "bridge.import", "project.setup", "project.open", "plot.batch", "takeoff.export", "takeoff.bill", "xref.attach", "xref.reload", "insert", "sheet.template.save", "sheet.template.apply", "sheet.template.delete", "sheet.template.list"].contains(&spec.id) {
            return Ok(());
        }
        let mut s = session();
        let r = s.execute(spec.id, &params);
        prop_assert!(!matches!(r, Err(EngineError::Internal(..))), "{} panicked (caught) on {params}: {r:?}", spec.id);
    }
}

// ---------------- 2 and 3: edit sequences ----------------

#[derive(Clone, Debug)]
enum Op {
    Line(i16, i16, i16, i16),
    Circle(i16, i16, i16),
    Move(usize, i16, i16),
    Copy(usize, i16, i16),
    Erase(usize),
    Rotate(usize, i16),
    Scale(usize, i16),
    Mirror(usize, i16, i16, i16, i16),
    Offset(usize, i16),
    Layer(usize),
    Color(usize, u8),
}

fn op() -> impl Strategy<Value = Op> {
    let c = || -2000i16..2000;
    prop_oneof![
        (c(), c(), c(), c()).prop_map(|(a, b, d, e)| Op::Line(a, b, d, e)),
        (c(), c(), -50i16..500).prop_map(|(x, y, r)| Op::Circle(x, y, r)),
        (any::<usize>(), c(), c()).prop_map(|(i, x, y)| Op::Move(i, x, y)),
        (any::<usize>(), c(), c()).prop_map(|(i, x, y)| Op::Copy(i, x, y)),
        any::<usize>().prop_map(Op::Erase),
        (any::<usize>(), -400i16..400).prop_map(|(i, a)| Op::Rotate(i, a)),
        (any::<usize>(), -10i16..10).prop_map(|(i, k)| Op::Scale(i, k)),
        (any::<usize>(), c(), c(), c(), c()).prop_map(|(i, a, b, d, e)| Op::Mirror(i, a, b, d, e)),
        (any::<usize>(), -300i16..300).prop_map(|(i, d)| Op::Offset(i, d)),
        any::<usize>().prop_map(Op::Layer),
        (any::<usize>(), 0u8..=255).prop_map(|(i, c)| Op::Color(i, c)),
    ]
}

/// Run one operation; `None` when there is nothing to act on.
fn apply(s: &mut Session, op: &Op) -> Option<Result<Value, EngineError>> {
    let hs = handles(s);
    let pick = |i: usize| hs.get(i % hs.len().max(1)).cloned();
    Some(match op {
        Op::Line(a, b, c, d) => s.execute("line", &json!({"points": [[a, b], [c, d]]})),
        Op::Circle(x, y, r) => s.execute("circle", &json!({"center": [x, y], "radius": r})),
        Op::Move(i, x, y) => s.execute("move", &json!({"handles": [pick(*i)?], "delta": [x, y]})),
        Op::Copy(i, x, y) => s.execute("copy", &json!({"handles": [pick(*i)?], "delta": [x, y]})),
        Op::Erase(i) => s.execute("erase", &json!({"handles": [pick(*i)?]})),
        Op::Rotate(i, a) => s.execute("rotate", &json!({"handles": [pick(*i)?], "base": [0, 0], "angle": a})),
        Op::Scale(i, k) => s.execute("scale", &json!({"handles": [pick(*i)?], "base": [0, 0], "factor": k})),
        Op::Mirror(i, a, b, c, d) => s.execute("mirror", &json!({"handles": [pick(*i)?], "p1": [a, b], "p2": [c, d]})),
        Op::Offset(i, d) => s.execute("offset", &json!({"handle": pick(*i)?, "distance": d, "side": [0, 0]})),
        Op::Layer(i) => s.execute("properties.set", &json!({"handles": [pick(*i)?], "layer": "A"})),
        Op::Color(i, c) => s.execute("properties.set", &json!({"handles": [pick(*i)?], "color": c})),
    })
}

proptest! {
    #[test]
    fn a_failed_command_changes_nothing(ops in prop::collection::vec(op(), 1..12)) {
        let mut s = session();
        for o in &ops {
            let before = state(&s);
            let Some(r) = apply(&mut s, o) else { continue };
            match r {
                Err(EngineError::Internal(..)) => prop_assert!(false, "{o:?} panicked (caught)"),
                Err(_) => prop_assert!(state(&s) == before, "{o:?} failed but changed the drawing"),
                Ok(_) => {}
            }
        }
    }

    #[test]
    fn undoing_everything_restores_the_drawing_and_redoing_restores_the_result(ops in prop::collection::vec(op(), 1..10)) {
        let mut s = session();
        let original = state(&s);
        let undo_base = s.state().unwrap().undo.len();
        let mut done = 0usize;
        for o in &ops {
            let before = s.state().unwrap().undo.len();
            if let Some(Ok(_)) = apply(&mut s, o) && s.state().unwrap().undo.len() > before {
                done += 1;
            }
        }
        let edited = state(&s);
        for _ in 0..done {
            let r = s.execute("undo", &json!({}));
            prop_assert!(r.is_ok(), "undo failed: {r:?}");
        }
        prop_assert_eq!(s.state().unwrap().undo.len(), undo_base);
        prop_assert!(state(&s) == original, "undo did not restore the original");
        for _ in 0..done {
            let r = s.execute("redo", &json!({}));
            prop_assert!(r.is_ok(), "redo failed: {r:?}");
        }
        prop_assert!(state(&s) == edited, "redo did not restore the edited drawing");
    }

    #[test]
    fn moving_and_moving_back_restores_the_geometry(dx in -5000i16..5000, dy in -5000i16..5000, i in any::<usize>()) {
        let mut s = session();
        let h = handles(&s)[i % handles(&s).len()].clone();
        let before = state(&s).0;
        s.execute("move", &json!({"handles": [h], "delta": [dx, dy]})).unwrap();
        s.execute("move", &json!({"handles": [h], "delta": [-dx, -dy]})).unwrap();
        let after = state(&s).0;
        prop_assert_eq!(before.len(), after.len());
        // Compare through the JSON form with numbers rounded: moves go through floating point.
        let round = |v: &aad_engine::doc::Entity| {
            let mut t = serde_json::to_string(v).unwrap();
            // Strip the digits beyond 6 decimals so 1e-12 rounding noise does not matter.
            let mut out = String::new();
            let mut it = t.drain(..).peekable();
            let mut frac = 0;
            let mut in_frac = false;
            while let Some(c) = it.next() {
                if in_frac && c.is_ascii_digit() {
                    frac += 1;
                    if frac <= 6 { out.push(c); }
                    continue;
                }
                in_frac = c == '.' && it.peek().is_some_and(|n| n.is_ascii_digit());
                frac = 0;
                out.push(c);
            }
            out.replace("-0.000000", "0.000000").replace("-0.0", "0.0")
        };
        for (a, b) in before.iter().zip(after.iter()) {
            prop_assert_eq!(round(a), round(b));
        }
    }
}
